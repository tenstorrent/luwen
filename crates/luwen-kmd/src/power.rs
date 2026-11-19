// SPDX-FileCopyrightText: © 2026 Tenstorrent Inc.
// SPDX-License-Identifier: Apache-2.0

//! Power domains a power-aware handle must hold before touching tiles that
//! firmware can clock-gate.
//!
//! Luwen opens devices with `O_APPEND`, which tells tt-kmd that the handle will
//! request the power domains it needs (`TENSTORRENT_IOCTL_SET_POWER_STATE`). A
//! NoC access that terminates in a gated Tensix, L2CPU or GDDR tile never
//! completes and drops the device off the bus. Bit values match the ioctl's
//! `power_flags`.

use luwen_def::Arch;
use nix::errno::Errno;

use crate::Tlb;

/// Maximum AI clock. Never needed for a tile to be reachable.
pub(crate) const AICLK: u8 = 1 << 0;
/// MRISC PHY wakeup (GDDR).
pub(crate) const MRISC: u8 = 1 << 1;
pub(crate) const TENSIX: u8 = 1 << 2;
pub(crate) const L2CPU: u8 = 1 << 3;
/// Blackhole firmware treats this bit as reserved.
pub(crate) const PCIE: u8 = 1 << 4;

/// Requested before a NoC access to a tile that may be gated (UMD's "busy" set).
pub(crate) const NOC_ACCESS: u8 = MRISC | TENSIX | L2CPU;

/// True unless `tlb` is a NoC0 unicast to a tile that no power flag can gate.
///
/// Broadcast and multicast (gated Tensix stay broadcast receivers), NoC1 (its
/// coordinates are ambiguous without knowing whether translation is on) and
/// unknown coordinates are treated as gateable.
pub(crate) fn tlb_needs_power(arch: Arch, tlb: &Tlb) -> bool {
    let always_on = match arch {
        Arch::Blackhole => bh_always_on_noc0(tlb.x_end, tlb.y_end),
        Arch::Wormhole => wh_always_on_noc0(tlb.x_end, tlb.y_end),
        // No power management.
        #[allow(deprecated)]
        Arch::Grayskull => return false,
    };
    tlb.mcast || tlb.noc_sel != 0 || !always_on
}

/// Blackhole NoC0 tiles that are never clock-gated: ARC, PCIe, Ethernet,
/// security and router-only, in raw and translated coordinates.
fn bh_always_on_noc0(x: u8, y: u8) -> bool {
    matches!(
        (x, y),
        // Row 0: router-only, PCIe 2-0 and 11-0, ARC 8-0. Row 1: Ethernet and
        // router-only 8-1. The GDDR columns x=0 and x=9 are excluded.
        // Translation never remaps X on rows 0 and 1, and Y is identity below 12.
        (1..=8 | 10..=16, 0 | 1)
            // Column 8: security 8-2 and router-only. Excludes L2CPU 8-3/5/7/9
            // (UMD blackhole_implementation.hpp; the tt-metal SoC descriptor lists
            // them as router_only).
            | (8, 2 | 4 | 6 | 8 | 10 | 11)
            // Translated only: PCIe 19-24, Ethernet 20-25..31-25, security 8-30.
            | (19, 24)
            | (20..=31, 25)
            | (8, 30)
    )
}

/// Wormhole NoC0 tiles that are never clock-gated: ARC 0-10, PCIe 0-3,
/// router-only 0-2/0-4/0-8/0-9 and the Ethernet rows 0 and 6.
fn wh_always_on_noc0(x: u8, y: u8) -> bool {
    matches!(
        (x, y),
        (0, 2 | 3 | 4 | 8 | 9 | 10) | (1..=4 | 6..=9, 0 | 6)
    )
}

/// The request to send so that `current` also covers `needed`, or `None` if it
/// already does. Never drops bits.
pub(crate) fn merge_request(current: u8, needed: u8) -> Option<u8> {
    (current & needed != needed).then_some(current | needed)
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum OnRequestError {
    Proceed,
    Fail,
}

/// What to do when a power request fails.
pub(crate) fn on_request_error(arch: Arch, err: Errno) -> OnRequestError {
    match (arch, err) {
        // KMD or firmware without power management; such firmware never gates tiles.
        (_, Errno::EINVAL | Errno::ENOTTY | Errno::EOPNOTSUPP | Errno::EREMOTEIO) => {
            OnRequestError::Proceed
        }
        // Firmware that manages power but did not confirm the request: refusing
        // the access beats a completion timeout that drops the device.
        (Arch::Blackhole, _) => OnRequestError::Fail,
        // Wormhole firmware support varies; the request is best effort.
        _ => OnRequestError::Proceed,
    }
}

/// The handle's cached request after sending `bits` got `result`.
///
/// A request that firmware managing power did not confirm is not trusted: its
/// MRISC, Tensix and L2CPU bits are left out of the cache, so the next access to
/// a gateable tile sends them again. The caller's other bits are kept.
pub(crate) fn cache_after_send(arch: Arch, bits: u8, result: Result<(), Errno>) -> u8 {
    match result {
        Err(err) if on_request_error(arch, err) == OnRequestError::Fail => bits & !NOC_ACCESS,
        _ => bits,
    }
}

/// Before a NoC access to a gateable tile: if `cached` lacks any of
/// [`NOC_ACCESS`], sends `cached | NOC_ACCESS` and updates `cached`. Returns the
/// bits sent and the result of the last send, or `None` if nothing was sent.
///
/// A timeout that would fail the access is retried once. Firmware wakes MRISC
/// last, and that wakeup has its own 1 s timeout, as long as tt-kmd's wait for
/// the reply.
pub(crate) fn request_for_access(
    arch: Arch,
    cached: &mut u8,
    mut send: impl FnMut(u8) -> Result<(), Errno>,
) -> Option<(u8, Result<(), Errno>)> {
    let bits = merge_request(*cached, NOC_ACCESS)?;
    let mut result = send(bits);
    if result == Err(Errno::ETIMEDOUT)
        && on_request_error(arch, Errno::ETIMEDOUT) == OnRequestError::Fail
    {
        result = send(bits);
    }
    *cached = cache_after_send(arch, bits, result);
    Some((bits, result))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unicast(x: u8, y: u8, noc_sel: u8) -> Tlb {
        Tlb {
            x_end: x,
            y_end: y,
            noc_sel,
            ..Default::default()
        }
    }

    fn needs(arch: Arch, x: u8, y: u8) -> bool {
        tlb_needs_power(arch, &unicast(x, y, 0))
    }

    #[test]
    fn bh_grid_always_on_is_exactly_36() {
        let mut always_on = vec![];
        for y in 0..12 {
            for x in 0..17 {
                if !needs(Arch::Blackhole, x, y) {
                    always_on.push((x, y));
                }
            }
        }
        let mut expected = vec![];
        for y in 0..=1 {
            for x in (1..=8).chain(10..=16) {
                expected.push((x, y));
            }
        }
        for y in [2, 4, 6, 8, 10, 11] {
            expected.push((8, y));
        }
        always_on.sort();
        expected.sort();
        assert_eq!(always_on, expected);
        // 140 Tensix + 24 GDDR + 4 L2CPU need power.
        assert_eq!(17 * 12 - always_on.len(), 168);

        for y in 2..=11 {
            for x in (1..=7).chain(10..=16) {
                assert!(needs(Arch::Blackhole, x, y), "Tensix ({x}, {y})");
            }
        }
        for y in 0..12 {
            assert!(needs(Arch::Blackhole, 0, y), "GDDR (0, {y})");
            assert!(needs(Arch::Blackhole, 9, y), "GDDR (9, {y})");
        }
        for y in [3, 5, 7, 9] {
            assert!(needs(Arch::Blackhole, 8, y), "L2CPU (8, {y})");
        }
    }

    #[test]
    fn bh_known_targets() {
        // ARC, PCIe, security.
        for (x, y) in [(8, 0), (2, 0), (11, 0), (8, 2)] {
            assert!(!needs(Arch::Blackhole, x, y), "({x}, {y})");
        }
        // All 14 Ethernet tiles.
        for x in [1, 16, 2, 15, 3, 14, 4, 13, 5, 12, 6, 11, 7, 10] {
            assert!(!needs(Arch::Blackhole, x, 1), "ETH ({x}, 1)");
        }
        // Tensix (1, 2) soft reset register: hung the device while gated.
        let tlb = Tlb {
            x_end: 1,
            y_end: 2,
            noc_sel: 0,
            local_offset: 0xFFB121B0,
            ..Default::default()
        };
        assert!(tlb_needs_power(Arch::Blackhole, &tlb));
    }

    #[test]
    fn bh_translated() {
        for (x, y) in [(19, 24), (20, 25), (31, 25), (8, 30)] {
            assert!(!needs(Arch::Blackhole, x, y), "({x}, {y})");
        }
        // GDDR 17-12..18-23, L2CPU 8-26..8-29, out-of-range and unknown targets.
        let mut gateable = vec![(32, 25), (33, 25), (17, 12), (18, 23), (8, 24)];
        gateable.extend([(17, 0), (40, 40), (255, 255)]);
        gateable.extend((26..=29).map(|y| (8, y)));
        for (x, y) in gateable {
            assert!(needs(Arch::Blackhole, x, y), "({x}, {y})");
        }
    }

    #[test]
    fn wh_grid_always_on_is_exactly_22() {
        let mut always_on = vec![];
        for y in 0..12 {
            for x in 0..10 {
                if !needs(Arch::Wormhole, x, y) {
                    always_on.push((x, y));
                }
            }
        }
        assert_eq!(always_on.len(), 22);
        // 80 Tensix + 18 GDDR need power.
        assert_eq!(10 * 12 - always_on.len(), 98);

        // ARC, PCIe, router-only.
        for y in [10, 3, 2, 4, 8, 9] {
            assert!(!needs(Arch::Wormhole, 0, y), "(0, {y})");
        }
        // Ethernet tiles, as used by tt-smi/tt-flash and Luwen's own (4, 6) and (9, 0).
        for y in [0, 6] {
            for x in [9, 1, 8, 2, 7, 3, 6, 4] {
                assert!(!needs(Arch::Wormhole, x, y), "ETH ({x}, {y})");
            }
        }
        for (x, y) in [(1, 1), (0, 0), (5, 4), (0, 11)] {
            assert!(needs(Arch::Wormhole, x, y), "({x}, {y})");
        }
    }

    #[test]
    fn modifiers_force_request() {
        for (arch, x, y) in [(Arch::Blackhole, 8, 0), (Arch::Wormhole, 0, 10)] {
            let mcast = Tlb {
                mcast: true,
                ..unicast(x, y, 0)
            };
            assert!(tlb_needs_power(arch, &mcast));
        }
        // Luwen's Blackhole broadcast rectangle.
        let broadcast = Tlb {
            x_start: 0,
            y_start: 1,
            mcast: true,
            ..unicast(16, 11, 0)
        };
        assert!(tlb_needs_power(Arch::Blackhole, &broadcast));
        // NoC1: (8, 11) is ARC mirrored, (8, 0) is ARC unmirrored.
        assert!(tlb_needs_power(Arch::Blackhole, &unicast(8, 11, 1)));
        assert!(tlb_needs_power(Arch::Blackhole, &unicast(8, 0, 1)));
    }

    #[test]
    #[allow(deprecated)]
    fn grayskull_never_requests() {
        assert!(!tlb_needs_power(Arch::Grayskull, &unicast(1, 1, 0)));
        let mcast = Tlb {
            mcast: true,
            ..unicast(1, 1, 1)
        };
        assert!(!tlb_needs_power(Arch::Grayskull, &mcast));
    }

    #[test]
    fn merge_request_truth_table() {
        assert_eq!(NOC_ACCESS, 0xe);
        assert_eq!(merge_request(0, NOC_ACCESS), Some(0xe));
        assert_eq!(merge_request(0xe, NOC_ACCESS), None);
        assert_eq!(merge_request(0x1f, NOC_ACCESS), None);
        assert_eq!(merge_request(AICLK, NOC_ACCESS), Some(0xf));
        assert_eq!(merge_request(AICLK | PCIE, NOC_ACCESS), Some(0x1f));
        assert_eq!(merge_request(MRISC | TENSIX, NOC_ACCESS), Some(0xe));
        for current in 0..=0x1f {
            assert_eq!(merge_request(current, 0), None);
        }
    }

    #[test]
    fn request_error_policy() {
        use OnRequestError::{Fail, Proceed};
        for (arch, err, action) in [
            (Arch::Blackhole, Errno::ETIMEDOUT, Fail),
            (Arch::Blackhole, Errno::EIO, Fail),
            (Arch::Blackhole, Errno::EREMOTEIO, Proceed),
            (Arch::Blackhole, Errno::EOPNOTSUPP, Proceed),
            (Arch::Blackhole, Errno::ENOTTY, Proceed),
            (Arch::Wormhole, Errno::ETIMEDOUT, Proceed),
            (Arch::Wormhole, Errno::EREMOTEIO, Proceed),
        ] {
            assert_eq!(on_request_error(arch, err), action, "{arch:?} {err}");
        }
    }

    /// Runs `request_for_access` with a sender that records each request and
    /// answers from `replies`.
    fn access(
        arch: Arch,
        cached: &mut u8,
        replies: &[Result<(), Errno>],
    ) -> (Vec<u8>, Option<Result<(), Errno>>) {
        let mut sent = vec![];
        let mut replies = replies.iter().copied();
        let result = request_for_access(arch, cached, |bits| {
            sent.push(bits);
            replies.next().expect("unexpected power request")
        });
        (sent, result.map(|(_, result)| result))
    }

    #[test]
    fn access_requests_once_on_top_of_explicit_request() {
        let bh = Arch::Blackhole;
        let mut cached = 0;
        assert_eq!(
            access(bh, &mut cached, &[Ok(())]),
            (vec![0xe], Some(Ok(())))
        );
        assert_eq!(access(bh, &mut cached, &[]), (vec![], None));

        // set_power_state(Low) replaces the request; the next access raises it again.
        cached = cache_after_send(bh, 0, Ok(()));
        assert_eq!(
            access(bh, &mut cached, &[Ok(())]),
            (vec![0xe], Some(Ok(())))
        );
        // An explicit AICLK request is kept.
        cached = cache_after_send(bh, AICLK, Ok(()));
        assert_eq!(access(bh, &mut cached, &[Ok(())]).0, vec![0xf]);
        // set_power_state(High) already covers the access.
        cached = cache_after_send(bh, 0x1f, Ok(()));
        assert_eq!(access(bh, &mut cached, &[]), (vec![], None));
    }

    #[test]
    fn blackhole_retries_unconfirmed_requests() {
        use Errno::{EIO, ETIMEDOUT};
        let bh = Arch::Blackhole;
        // One timeout is retried within the access.
        let mut cached = 0;
        let replies = [Err(ETIMEDOUT), Ok(())];
        assert_eq!(
            access(bh, &mut cached, &replies),
            (vec![0xe, 0xe], Some(Ok(())))
        );
        assert_eq!(cached, 0xe);

        // Two timeouts, or another error, refuse the access; the next access sends again.
        cached = 0;
        let replies = [Err(ETIMEDOUT), Err(ETIMEDOUT)];
        assert_eq!(
            access(bh, &mut cached, &replies),
            (vec![0xe, 0xe], Some(Err(ETIMEDOUT)))
        );
        assert_eq!(
            access(bh, &mut cached, &[Err(EIO)]),
            (vec![0xe], Some(Err(EIO)))
        );
        assert_eq!(
            access(bh, &mut cached, &[Ok(())]),
            (vec![0xe], Some(Ok(())))
        );

        // A failed explicit request is not trusted either; its other bits are kept.
        cached = cache_after_send(bh, 0x1f, Err(ETIMEDOUT));
        assert_eq!(cached, AICLK | PCIE);
        assert_eq!(
            access(bh, &mut cached, &[Ok(())]),
            (vec![0x1f], Some(Ok(())))
        );
    }

    #[test]
    fn unsupported_or_best_effort_requests_are_sent_once() {
        use Errno::{EINVAL, EIO, EOPNOTSUPP, EREMOTEIO, ETIMEDOUT};
        for (arch, err) in [
            (Arch::Blackhole, EINVAL),
            (Arch::Blackhole, EREMOTEIO),
            (Arch::Blackhole, EOPNOTSUPP),
            (Arch::Wormhole, ETIMEDOUT),
            (Arch::Wormhole, EIO),
        ] {
            let mut cached = 0;
            let first = access(arch, &mut cached, &[Err(err)]);
            assert_eq!(first, (vec![0xe], Some(Err(err))), "{arch:?} {err}");
            assert_eq!(
                access(arch, &mut cached, &[]),
                (vec![], None),
                "{arch:?} {err}"
            );
            assert_eq!(
                cache_after_send(arch, 0x1f, Err(err)),
                0x1f,
                "{arch:?} {err}"
            );
        }
    }
}

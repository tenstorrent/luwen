%bcond_without check
#% bcond_with check

%global crate luwen
%global path_luwen_api %{cargo_registry}/luwen-api-%{version_no_tilde}
%global path_luwen_pci %{cargo_registry}/luwen-pci-%{version_no_tilde}
%global path_luwen_kmd %{cargo_registry}/luwen-kmd-%{version_no_tilde}

Name:           rust-luwen
Version:        0.4.8
Release:        %autorelease
Summary:        High-level interface for safe and efficient access Tenstorrent AI accelerators

License:        Apache-2.0
URL:            https://crates.io/crates/luwen
Source: 	%{name}-%{version}.tar.gz

BuildRequires:  cargo-rpm-macros >= 24
BuildRequires:  python%{python3_pkgversion}-devel
#BuildRequires:  rust-bincode+default-devel
#BuildRequires:  rust-bitfield+default-devel
#BuildRequires:  rust-cbindgen+default-devel
#BuildRequires:  rust-clap+default-devel
#BuildRequires:  rust-clap+derive-devel
#BuildRequires:  rust-indicatif+default-devel
#BuildRequires:  rust-memmap2+default-devel
#BuildRequires:  rust-nix+default-devel
#BuildRequires:  rust-nom+default-devel
#BuildRequires:  rust-once_cell+default-devel
#BuildRequires:  rust-prometheus+default-devel
#BuildRequires:  rust-prometheus+process-devel
#BuildRequires:  rust-pyo3+default-devel
#BuildRequires:  rust-pyo3+extension-module-devel
#BuildRequires:  rust-pyo3+multiple-pymethods-devel
#BuildRequires:  rust-rand+default-devel
#BuildRequires:  rust-rust-embed+default-devel
#BuildRequires:  rust-rust-embed+interpolate-folder-path-devel
#BuildRequires:  rust-serde+default-devel
#BuildRequires:  rust-serde+derive-devel
#BuildRequires:  rust-serde_yaml+default-devel
#BuildRequires:  rust-thiserror+default-devel
#BuildRequires:  rust-tracing+default-devel
#BuildRequires:  rust-memmap2_0.7+default-devel
#BuildRequires:  rust-nix0.26+default-devel
#BuildRequires:  rust-pyo3_0.19+default-devel
#BuildRequires:  rust-pyo3_0.19+extension-module-devel
#BuildRequires:  rust-pyo3_0.19+multiple-pymethods-devel

# we need to manipulate the Cargo.toml files in flight
BuildRequires:  tomcli
BuildRequires:  maturin

%global _description %{expand:
A high-level interface for safe and efficient access Tenstorrent AI
accelerators.}

%description
%{_description}

%package     -n %{crate}
Summary:        %{summary}
# FIXME: paste output of %%cargo_license_summary here
#License:        # FIXME
License:        Apache-2.0
# LICENSE.dependencies contains a full license breakdown

%description -n %{crate}
%{_description}

%files       -n %{crate}
%license LICENSE
%license LICENSE.dependencies
%doc README.md
%doc SUMMARY.md
%doc TODO.md
%{_bindir}/detect_test
%{_bindir}/druken_monkey
%{_bindir}/ethernet_benchmark
%{_bindir}/generate_names
%{_bindir}/luwen-cem
%{_bindir}/luwen-demo
%{_bindir}/reset-test
%{_bindir}/spi-test

############################
# rust-luwen-api
############################

%package     -n rust-luwen-api-devel
Summary:        Python bindings for the Tenstorrent Luwen library

%description -n rust-luwen-api-devel

This package contains library source intended for building other packages which
use the "internal_metrics" feature of the "%{crate}" crate.

%files       -n rust-luwen-api-devel
%{cargo_registry}/luwen-api-%{version_no_tilde}

############################
# rust-luwen-pci
############################

%package     -n rust-luwen-pci-devel
Summary:        Python bindings for the Tenstorrent Luwen library

%description -n rust-luwen-pci-devel

This package contains library source intended for building other packages which
use the "internal_metrics" feature of the "%{crate}" crate.

%files       -n rust-luwen-pci-devel
%{cargo_registry}/luwen-pci-%{version_no_tilde}

############################
# rust-luwen-kmd
############################

%package     -n rust-luwen-kmd-devel
Summary:        Python bindings for the Tenstorrent Luwen library

%description -n rust-luwen-kmd-devel

This package contains library source intended for building other packages which
use the "internal_metrics" feature of the "%{crate}" crate.

%files       -n rust-luwen-kmd-devel
%{cargo_registry}/luwen-kmd-%{version_no_tilde}

############################
# PyLuwen
############################

%package     -n python3-pyluwen
Summary:        Python bindings for the Tenstorrent Luwen library

%description -n python3-pyluwen
%{_description}

This package contains library source intended for building other packages which
use the "internal_metrics" feature of the "%{crate}" crate.

%files       -n python3-pyluwen
%{python3_sitearch}/pyluwen-*.dist-info
%{python3_sitearch}/pyluwen/

############################
# Luwen Test Binaries
############################
%package     -n luwen-test-bin
Summary:        Testing and Debug binaries associated with Luwen

%description -n luwen-test-bin
%{_description}

This is Testing and Debug binaries associated with Luwen

%files       -n luwen-test-bin
%{_bindir}/*
%{_exec_prefix}/lib/debug/usr/bin/*

############################
# Main package
############################

%prep
%autosetup -p1 
%cargo_prep

%generate_buildrequires
%cargo_generate_buildrequires

%build
# This builds everything but libluwen and pyluwen, the former has a bug, the later we build independently
%cargo_build '--workspace' '--exclude' 'libluwen' '--exclude' 'pyluwen'
%{cargo_license_summary}
%{cargo_license} > LICENSE.dependencies

# build pyluwen
cd bind/pyluwen
CFLAGS="${CFLAGS:-${RPM_OPT_FLAGS}}" \
LDFLAGS="${LDFLAGS:-${RPM_LD_FLAGS}}" \
maturin build --release %{?py_setup_args} %{?*}

%install
%cargo_install
#%{__cp} -av LICENSE % {crate_instdir}/LICENSE
#%{__cp} -av README.md % {crate_instdir}/README.md

#
# Install PyLuwen
#	Do this as a pip install, there's not really a 'better' way
#
(
	# this is all cribbed from py3_install macro
	cd bind/pyluwen
	/usr/bin/pip install . --root %{buildroot} --prefix %{_prefix}
	rm -rfv %{buildroot}%{_bindir}/__pycache__
)

mkdir -p %{buildroot}%{cargo_registry}

%{__cp} -av crates/luwen-api %{buildroot}%{path_luwen_api}
# Modify the existing Cargo.toml to remove the path for luwen-def as we are moving it out to it's own place on the system
tomcli \
	set \
	%{buildroot}%{path_luwen_api}/Cargo.toml \
	str \
	dependencies.luwen-def \
	"$( \
		tomcli \
		get \
		%{buildroot}%{path_luwen_api}/Cargo.toml \
		dependencies.luwen-def.version \
	)"
echo "--- luwen-api Cargo.toml ---"
cat %{buildroot}%{path_luwen_api}/Cargo.toml
echo "--- /luwen-api Cargo.toml ---"
%{__cp} -av crates/luwen-pci %{buildroot}%{path_luwen_pci}
# Modify the existing Cargo.toml to remove the path for luwen-def as we are moving it out to it's own place on the system

for x in luwen-def luwen-api luwen-kmd
do
	tomcli \
		set \
		%{buildroot}%{path_luwen_pci}/Cargo.toml \
		str \
		dependencies.${x} \
		"$( \
			tomcli \
			get \
			%{buildroot}%{path_luwen_pci}/Cargo.toml \
			dependencies.${x}.version \
		)"
done

%{__cp} -av crates/luwen-kmd %{buildroot}%{path_luwen_kmd}
# Modify the existing Cargo.toml to remove the path for luwen-def as we are moving it out to it's own place on the system

# luwen-def pathfix
tomcli \
	set \
	%{buildroot}%{path_luwen_kmd}/Cargo.toml \
	str \
	dependencies.luwen-def \
	"$( \
		tomcli \
		get \
		%{buildroot}%{path_luwen_kmd}/Cargo.toml \
		dependencies.luwen-def.version \
	)"

%if %{with check}
%check
%cargo_test
%endif

%changelog
%autochangelog
* Wed Apr 03 2024 John 'Warthog9' Hawley <jhawley@tenstorrent.com> 0.3.7-1
- new package built with tito

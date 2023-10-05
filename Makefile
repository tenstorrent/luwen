.PHONY: whl
whl:
ifndef DEST_DIR
	$(error DEST_DIR is undefined)
endif
	$(MAKE) -C crates/pyluwen whl
	ls target/wheels/pyluwen*.whl | xargs -I {} cp {} $(DEST_DIR)

.PHONY: dev-whl
dev-whl:
ifndef DEST_DIR
	$(error DEST_DIR is undefined)
endif
	$(MAKE) -C crates/pyluwen dev-whl
	ls target/wheels/pyluwen*.whl | xargs -I {} cp {} $(DEST_DIR)

.PHONY: syseng-release
syseng-release:
	$(MAKE) whl \
		DEST_DIR=~/work/syseng/src/t6ifc/t6py/packages/whl \
		PYTHON=python3.7

.PHONY: deb
deb:
	@if ! cargo --list | grep -q '^\s*deb\s*$$'; then \
        echo "Error: cargo-deb is not installed. Please install it using 'cargo install --locked cargo-deb'."; \
        exit 1; \
    fi
	cargo deb -p luwencpp --target x86_64-unknown-linux-gnu -v
	# sudo dpkg -i ./target/x86_64-unknown-linux-gnu/debian/luwencpp_0.1.0-1_amd64.deb

.PHONY: rpm
rpm:
	$(MAKE) -C crates/luwencpp rpm

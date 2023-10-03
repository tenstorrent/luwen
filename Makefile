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
	$(MAKE) -C crates/luwencpp deb

.PHONY: rpm
rpm:
	$(MAKE) -C crates/luwencpp rpm

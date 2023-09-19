.PHONY: whl
whl:
	$(MAKE) -C crates/pyluwen whl

.PHONY: dev-whl
dev-whl:
	$(MAKE) -C crates/pyluwen dev-whl

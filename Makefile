.PHONY: whl
justbuild:
	$(MAKE) -C bind/pyluwen whl

whl: justbuild
ifndef DEST_DIR
	$(error DEST_DIR is undefined)
endif
	ls target/wheels/pyluwen*.whl | xargs -I {} cp {} $(DEST_DIR)

.PHONY: dev-whl
dev-whl:
ifndef DEST_DIR
	$(error DEST_DIR is undefined)
endif
	$(MAKE) -C bind/pyluwen dev-whl
	ls target/wheels/pyluwen*.whl | xargs -I {} cp {} $(DEST_DIR)

.PHONY: syseng-release
syseng-release:
	$(MAKE) whl \
		DEST_DIR=~/work/syseng/src/t6ifc/t6py/packages/whl \
		PYTHON=python3.7

.PHONY: flash-release
flash-release:
	$(MAKE) whl \
		DEST_DIR=~/work/tt-flash/pyluwen/whl \
		PYTHON=python3.7

.PHONY: tools-common-release
tools-common-release:
	$(MAKE) whl \
		DEST_DIR=~/tt-tools-common/pyluwen/whl \
		PYTHON=python3.7

.PHONY: deb
deb:
	@if ! cargo --list | grep -q '^\s*deb\s*$$'; then \
        echo "Error: cargo-deb is not installed. Please install it using 'cargo install --locked cargo-deb'."; \
        exit 1; \
    fi
	cargo deb -p libluwen --target x86_64-unknown-linux-gnu -v

.PHONY: rpm
rpm:
	$(MAKE) -C bind/libluwen rpm

.PHONY: upload-ci-docker
upload-ci-docker:
	docker login yyz-gitlab.local.tenstorrent.com:5005 -u drosen -p ${CONTAINER_ACCESS_TOKEN}
	docker build -t yyz-gitlab.local.tenstorrent.com:5005/syseng-platform/luwen/rust-ci-build -f ci/dockerfiles/Dockerfile ci/dockerfiles
	docker push yyz-gitlab.local.tenstorrent.com:5005/syseng-platform/luwen/rust-ci-build

clean:
	rm -rf \
		target \
		Cargo.lock

.PHONY: pyluwen-pyi
pyluwen-pyi:
	$(MAKE) -C bind/pyluwen build-pyi

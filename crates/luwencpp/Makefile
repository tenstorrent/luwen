# Define the Rust library name (without the lib prefix and file extension)
RUST_LIB_NAME = libluwen_umd

# Define the destination directory for the Rust library
DEST_DIR = open-umd/build/lib

.PHONY: init
init:
	$(MAKE) -C open-umd init

.PHONY: luwen
luwen: init
	cargo build --message-format=json | jq -r 'select(.reason == "compiler-artifact" and .executable == null).filenames[] | select(test("$(RUST_LIB_NAME)\\.(so|dylib|dll)"))' | xargs -I {} cp {} $(DEST_DIR)

build: luwen
	$(MAKE) -C open-umd build

test: luwen
	$(MAKE) -C open-umd test

run: luwen
	$(MAKE) -C open-umd run

clean:
	$(MAKE) -C open-umd clean

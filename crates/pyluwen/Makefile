PYTHON?=python3

.py03-env:
	$(PYTHON) -m venv .py03-env
	. .py03-env/bin/activate && pip install --upgrade pip
	. .py03-env/bin/activate && pip install -ve .

.PHONY: whl
whl: .py03-env
	. .py03-env/bin/activate && maturin build --release

.PHONY: dev-whl
dev-whl: .py03-env
	. .py03-env/bin/activate && maturin build

.PHONY: build-pyi
build-pyi:
	# rm -rf .py03-env
	# $(MAKE) .py03-env
	. .py03-env/bin/activate && python stub.py

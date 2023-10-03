PYTHON?=python3

.py03-env:
	$(PYTHON) -m venv .py03-env
	. .py03-env/bin/activate && pip install maturin

.PHONY: whl
whl: .py03-env
	. .py03-env/bin/activate && maturin build --release

.PHONY: dev-whl
dev-whl: .py03-env
	. .py03-env/bin/activate && maturin build

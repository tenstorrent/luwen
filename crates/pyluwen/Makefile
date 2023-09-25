.py03-env:
	python3 -m venv .py03-env
	. .py03-env/bin/activate && pip install maturin

.PHONY: whl
whl: .py03-env
	. .py03-env/bin/activate && maturin build --release --strip

.PHONY: dev-whl
dev-whl: .py03-env
	. .py03-env/bin/activate && maturin build

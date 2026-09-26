.PHONY: check test lint bench cov
check:
	./check.sh
test:
	python3 -m pytest -q
lint:
	ruff check fermium tests
cov:
	python3 -m pytest -q --cov=fermium --cov-report=term-missing
bench:
	python3 benchmarks/run.py

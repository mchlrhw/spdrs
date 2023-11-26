run-e2es:
	cargo test --features e2e

run-e2e-server:
	python -m http.server -d resources/test-data/e2e-pages 2> /dev/null &

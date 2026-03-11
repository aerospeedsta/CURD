PYTHON ?= python3

.PHONY: package test

test:
	@printf '{"input":"Auth"}' | $(PYTHON) tool.py

package:
	@echo "Copy tool.py into payload/bin/curd-demo-tool and package with curd-plugin-pack."

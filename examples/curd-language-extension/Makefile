# Reference build wrapper for the CURD language extension payload.

TREE_SITTER_DIR ?= ../tree-sitter-curd
PAYLOAD_DIR ?= payload

all: payload

payload:
	mkdir -p $(PAYLOAD_DIR)/lib
	mkdir -p $(PAYLOAD_DIR)/queries
	cp $(TREE_SITTER_DIR)/queries/curd.scm $(PAYLOAD_DIR)/queries/curd.scm
	@echo "Place the compiled tree-sitter-curd shared library into $(PAYLOAD_DIR)/lib/tree-sitter-curd.dylib"

clean:
	rm -rf $(PAYLOAD_DIR)

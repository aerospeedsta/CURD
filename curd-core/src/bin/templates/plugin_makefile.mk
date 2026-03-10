# CURD Tree-sitter Plugin Makefile Template
# Use this to compile a C/C++ tree-sitter grammar into a .so/.dylib for CURD

CC ?= cc
CXX ?= c++
CFLAGS ?= -O3 -fPIC -Isrc
CXXFLAGS ?= -O3 -fPIC -Isrc

UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
	SHARED_EXT = dylib
	LDFLAGS = -dynamiclib
else
	SHARED_EXT = so
	LDFLAGS = -shared
endif

all: plugin

plugin: src/parser.c src/scanner.c
	$(CC) $(CFLAGS) -c src/parser.c -o parser.o
	$(CXX) $(CXXFLAGS) -c src/scanner.c -o scanner.o
	$(CXX) $(LDFLAGS) parser.o scanner.o -o tree-sitter-plugin.$(SHARED_EXT)

clean:
	rm -f *.o *.$(SHARED_EXT)

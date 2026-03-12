# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_curd_global_optspecs
	string join \n h/help V/version
end

function __fish_curd_needs_command
	# Figure out if the current invocation already has a command.
	set -l cmd (commandline -opc)
	set -e cmd[1]
	argparse -s (__fish_curd_global_optspecs) -- $cmd 2>/dev/null
	or return
	if set -q argv[1]
		# Also print the command, so this can be used to figure out what it is.
		echo $argv[1]
		return 1
	end
	return 0
end

function __fish_curd_using_subcommand
	set -l cmd (__fish_curd_needs_command)
	test -z "$cmd"
	and return 1
	contains -- $cmd[1] $argv
end

complete -c curd -n "__fish_curd_needs_command" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_needs_command" -s V -l version -d 'Print version'
complete -c curd -n "__fish_curd_needs_command" -a "mcp" -d 'Start the Model Context Protocol (MCP) server over stdin/stdout'
complete -c curd -n "__fish_curd_needs_command" -a "init-agent" -d 'Initialize and authorize a new agent keypair, auto-configuring a specified harness'
complete -c curd -n "__fish_curd_needs_command" -a "ina" -d 'Initialize and authorize a new agent keypair, auto-configuring a specified harness'
complete -c curd -n "__fish_curd_needs_command" -a "agt" -d 'Initialize and authorize a new agent keypair, auto-configuring a specified harness'
complete -c curd -n "__fish_curd_needs_command" -a "doctor" -d 'Run built-in self diagnostics for indexing and regressions'
complete -c curd -n "__fish_curd_needs_command" -a "dct" -d 'Run built-in self diagnostics for indexing and regressions'
complete -c curd -n "__fish_curd_needs_command" -a "build" -d 'Build via CURD control plane (adapter-based dry-run/execute)'
complete -c curd -n "__fish_curd_needs_command" -a "bld" -d 'Build via CURD control plane (adapter-based dry-run/execute)'
complete -c curd -n "__fish_curd_needs_command" -a "b" -d 'Build via CURD control plane (adapter-based dry-run/execute)'
complete -c curd -n "__fish_curd_needs_command" -a "hook" -d 'Print a shell hook to implicitly route standard commands (like \'make\') through CURD'
complete -c curd -n "__fish_curd_needs_command" -a "hok" -d 'Print a shell hook to implicitly route standard commands (like \'make\') through CURD'
complete -c curd -n "__fish_curd_needs_command" -a "diff" -d 'Compare symbols at AST level'
complete -c curd -n "__fish_curd_needs_command" -a "dif" -d 'Compare symbols at AST level'
complete -c curd -n "__fish_curd_needs_command" -a "refactor" -d 'Semantic refactoring engine'
complete -c curd -n "__fish_curd_needs_command" -a "ref" -d 'Semantic refactoring engine'
complete -c curd -n "__fish_curd_needs_command" -a "context" -d 'Manage external workspace contexts (Read-Only or Semantic linking)'
complete -c curd -n "__fish_curd_needs_command" -a "ctx" -d 'Manage external workspace contexts (Read-Only or Semantic linking)'
complete -c curd -n "__fish_curd_needs_command" -a "init" -d 'Initialize CURD workspace: auto-detect build system, create .curd/ directory'
complete -c curd -n "__fish_curd_needs_command" -a "ini" -d 'Initialize CURD workspace: auto-detect build system, create .curd/ directory'
complete -c curd -n "__fish_curd_needs_command" -a "config" -d 'Manage CURD configuration and policies'
complete -c curd -n "__fish_curd_needs_command" -a "cfg" -d 'Manage CURD configuration and policies'
complete -c curd -n "__fish_curd_needs_command" -a "plugin-language" -d 'Install, remove, or list signed language plugins (.curdl)'
complete -c curd -n "__fish_curd_needs_command" -a "plang" -d 'Install, remove, or list signed language plugins (.curdl)'
complete -c curd -n "__fish_curd_needs_command" -a "plugin-tool" -d 'Install, remove, or list signed tool plugins (.curdt)'
complete -c curd -n "__fish_curd_needs_command" -a "ptool" -d 'Install, remove, or list signed tool plugins (.curdt)'
complete -c curd -n "__fish_curd_needs_command" -a "plugin-trust" -d 'Manage trusted signing keys for CURD plugin packages'
complete -c curd -n "__fish_curd_needs_command" -a "ptrust" -d 'Manage trusted signing keys for CURD plugin packages'
complete -c curd -n "__fish_curd_needs_command" -a "detach" -d 'Soft detach CURD from the current workspace (removes git hooks and scrubs scripts)'
complete -c curd -n "__fish_curd_needs_command" -a "det" -d 'Soft detach CURD from the current workspace (removes git hooks and scrubs scripts)'
complete -c curd -n "__fish_curd_needs_command" -a "delete" -d 'Permanently delete CURD from the current workspace by removing the .curd/ directory'
complete -c curd -n "__fish_curd_needs_command" -a "del" -d 'Permanently delete CURD from the current workspace by removing the .curd/ directory'
complete -c curd -n "__fish_curd_needs_command" -a "status" -d 'Print a summary of the current workspace state (index stats, shadow changes, etc.)'
complete -c curd -n "__fish_curd_needs_command" -a "st" -d 'Print a summary of the current workspace state (index stats, shadow changes, etc.)'
complete -c curd -n "__fish_curd_needs_command" -a "sts" -d 'Print a summary of the current workspace state (index stats, shadow changes, etc.)'
complete -c curd -n "__fish_curd_needs_command" -a "log" -d 'Tail the agent\'s mutation and execution history'
complete -c curd -n "__fish_curd_needs_command" -a "repl" -d 'Start the interactive CURD REPL for semantic exploration'
complete -c curd -n "__fish_curd_needs_command" -a "rpl" -d 'Start the interactive CURD REPL for semantic exploration'
complete -c curd -n "__fish_curd_needs_command" -a "run" -d 'Run a .curd script by compiling it to the current DSL IR'
complete -c curd -n "__fish_curd_needs_command" -a "test" -d 'Run semantic integrity tests on the symbol graph'
complete -c curd -n "__fish_curd_needs_command" -a "tst" -d 'Run semantic integrity tests on the symbol graph'
complete -c curd -n "__fish_curd_needs_command" -a "search" -d 'Semantic search across the indexed graph'
complete -c curd -n "__fish_curd_needs_command" -a "sch" -d 'Semantic search across the indexed graph'
complete -c curd -n "__fish_curd_needs_command" -a "s" -d 'Semantic search across the indexed graph'
complete -c curd -n "__fish_curd_needs_command" -a "graph" -d 'Explore the caller/callee dependency graph of a symbol'
complete -c curd -n "__fish_curd_needs_command" -a "g" -d 'Explore the caller/callee dependency graph of a symbol'
complete -c curd -n "__fish_curd_needs_command" -a "read" -d 'Read a file or semantic symbol exactly as the engine parses it'
complete -c curd -n "__fish_curd_needs_command" -a "red" -d 'Read a file or semantic symbol exactly as the engine parses it'
complete -c curd -n "__fish_curd_needs_command" -a "edit" -d 'Manually mutate a file or symbol via the AST-aware EditEngine'
complete -c curd -n "__fish_curd_needs_command" -a "edt" -d 'Manually mutate a file or symbol via the AST-aware EditEngine'
complete -c curd -n "__fish_curd_needs_command" -a "e" -d 'Manually mutate a file or symbol via the AST-aware EditEngine'
complete -c curd -n "__fish_curd_needs_command" -a "diagram" -d 'Generate diagrams from the symbol graph'
complete -c curd -n "__fish_curd_needs_command" -a "dia" -d 'Generate diagrams from the symbol graph'
complete -c curd -n "__fish_curd_needs_command" -a "session" -d 'Manage manual ShadowStore transaction sessions'
complete -c curd -n "__fish_curd_needs_command" -a "ses" -d 'Manage manual ShadowStore transaction sessions'
complete -c curd -n "__fish_curd_needs_command" -a "plan" -d 'Manage, inspect, and execute saved AI plans'
complete -c curd -n "__fish_curd_needs_command" -a "pln" -d 'Manage, inspect, and execute saved AI plans'
complete -c curd -n "__fish_curd_needs_command" -a "p" -d 'Manage, inspect, and execute saved AI plans'
complete -c curd -n "__fish_curd_needs_command" -a "index-worker"
complete -c curd -n "__fish_curd_needs_command" -a "completions" -d 'Generate shell completion scripts'
complete -c curd -n "__fish_curd_needs_command" -a "man" -d 'Generate man pages'
complete -c curd -n "__fish_curd_needs_command" -a "index" -d 'Alias for curd doctor indexing'
complete -c curd -n "__fish_curd_needs_command" -a "idx" -d 'Alias for curd doctor indexing'
complete -c curd -n "__fish_curd_needs_command" -a "h" -d 'Show help for CURD'
complete -c curd -n "__fish_curd_needs_command" -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand mcp" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand init-agent" -s n -l name -d 'Optional: Identifier for the agent (e.g., \'alpha\', \'claude_coder\'). Use commas for multiple names' -r
complete -c curd -n "__fish_curd_using_subcommand init-agent" -s r -l harness -d 'Optional: Target harness to configure (gemini, cursor, claude_desktop, claude_code). Auto-detects if omitted' -r
complete -c curd -n "__fish_curd_using_subcommand init-agent" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ina" -s n -l name -d 'Optional: Identifier for the agent (e.g., \'alpha\', \'claude_coder\'). Use commas for multiple names' -r
complete -c curd -n "__fish_curd_using_subcommand ina" -s r -l harness -d 'Optional: Target harness to configure (gemini, cursor, claude_desktop, claude_code). Auto-detects if omitted' -r
complete -c curd -n "__fish_curd_using_subcommand ina" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand agt" -s n -l name -d 'Optional: Identifier for the agent (e.g., \'alpha\', \'claude_coder\'). Use commas for multiple names' -r
complete -c curd -n "__fish_curd_using_subcommand agt" -s r -l harness -d 'Optional: Target harness to configure (gemini, cursor, claude_desktop, claude_code). Auto-detects if omitted' -r
complete -c curd -n "__fish_curd_using_subcommand agt" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand doctor" -l max-total-ms -d 'Optional threshold: fail when index total_ms exceeds this value' -r
complete -c curd -n "__fish_curd_using_subcommand doctor" -l max-parse-fail -d 'Optional threshold: fail when parse_fail count exceeds this value' -r
complete -c curd -n "__fish_curd_using_subcommand doctor" -l max-no-symbols-ratio -d 'Optional threshold: fail when no_symbols/total_files exceeds this ratio (0.0..1.0)' -r
complete -c curd -n "__fish_curd_using_subcommand doctor" -l max-skipped-large-ratio -d 'Optional threshold: fail when skipped_large/total_files exceeds this ratio (0.0..1.0)' -r
complete -c curd -n "__fish_curd_using_subcommand doctor" -l min-coverage-ratio -d 'Optional threshold: fail when coverage_ratio is below this value (0.0..1.0)' -r
complete -c curd -n "__fish_curd_using_subcommand doctor" -l require-coverage-state -d 'Optional threshold: fail when coverage state does not match this exactly (e.g. \'full\')' -r
complete -c curd -n "__fish_curd_using_subcommand doctor" -l min-symbol-count -d 'Optional threshold: fail when total symbols found is below this count' -r
complete -c curd -n "__fish_curd_using_subcommand doctor" -l min-symbols-per-k-files -d 'Optional threshold: fail when symbol density is below this value' -r
complete -c curd -n "__fish_curd_using_subcommand doctor" -l min-overlap-with-full -d 'Optional threshold: fail when overlap with full index is below this value' -r
complete -c curd -n "__fish_curd_using_subcommand doctor" -l report-out -d 'Write report as JSON to this path' -r -F
complete -c curd -n "__fish_curd_using_subcommand doctor" -l profile -d 'The profile of thresholds to apply (\'ci_fast\' or \'ci_strict\'). Overrides config' -r
complete -c curd -n "__fish_curd_using_subcommand doctor" -l index-mode -d 'Optional index mode for this doctor run (e.g. \'lazy\', \'full\')' -r -f -a "lazy\t''
full\t''"
complete -c curd -n "__fish_curd_using_subcommand doctor" -l index-scope -d 'Optional index scope for this doctor run' -r -f -a "auto\t''
forced\t''"
complete -c curd -n "__fish_curd_using_subcommand doctor" -l index-max-file-size -d 'Optional max file size in bytes for this doctor run' -r
complete -c curd -n "__fish_curd_using_subcommand doctor" -l index-large-file-policy -d 'Optional large file policy for this doctor run' -r -f -a "skip\t''
skeleton\t''
full\t''"
complete -c curd -n "__fish_curd_using_subcommand doctor" -l index-execution -d 'Optional index execution model for this doctor run' -r -f -a "multithreaded\t''
multiprocess\t''
singlethreaded\t''"
complete -c curd -n "__fish_curd_using_subcommand doctor" -l index-chunk-size -d 'Optional chunk size for multiprocess mode' -r
complete -c curd -n "__fish_curd_using_subcommand doctor" -l strict -d 'Fail with non-zero status when warnings/findings are present'
complete -c curd -n "__fish_curd_using_subcommand doctor" -l parity-rerun -d 'Re-run the index build twice and ensure symbol counts and hashes match exactly'
complete -c curd -n "__fish_curd_using_subcommand doctor" -l compare-with-full -d 'Compare index contents against a \'full\' mode run'
complete -c curd -n "__fish_curd_using_subcommand doctor" -l profile-index -d 'Run indexing multiple times to generate a performance profile'
complete -c curd -n "__fish_curd_using_subcommand doctor" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand dct" -l max-total-ms -d 'Optional threshold: fail when index total_ms exceeds this value' -r
complete -c curd -n "__fish_curd_using_subcommand dct" -l max-parse-fail -d 'Optional threshold: fail when parse_fail count exceeds this value' -r
complete -c curd -n "__fish_curd_using_subcommand dct" -l max-no-symbols-ratio -d 'Optional threshold: fail when no_symbols/total_files exceeds this ratio (0.0..1.0)' -r
complete -c curd -n "__fish_curd_using_subcommand dct" -l max-skipped-large-ratio -d 'Optional threshold: fail when skipped_large/total_files exceeds this ratio (0.0..1.0)' -r
complete -c curd -n "__fish_curd_using_subcommand dct" -l min-coverage-ratio -d 'Optional threshold: fail when coverage_ratio is below this value (0.0..1.0)' -r
complete -c curd -n "__fish_curd_using_subcommand dct" -l require-coverage-state -d 'Optional threshold: fail when coverage state does not match this exactly (e.g. \'full\')' -r
complete -c curd -n "__fish_curd_using_subcommand dct" -l min-symbol-count -d 'Optional threshold: fail when total symbols found is below this count' -r
complete -c curd -n "__fish_curd_using_subcommand dct" -l min-symbols-per-k-files -d 'Optional threshold: fail when symbol density is below this value' -r
complete -c curd -n "__fish_curd_using_subcommand dct" -l min-overlap-with-full -d 'Optional threshold: fail when overlap with full index is below this value' -r
complete -c curd -n "__fish_curd_using_subcommand dct" -l report-out -d 'Write report as JSON to this path' -r -F
complete -c curd -n "__fish_curd_using_subcommand dct" -l profile -d 'The profile of thresholds to apply (\'ci_fast\' or \'ci_strict\'). Overrides config' -r
complete -c curd -n "__fish_curd_using_subcommand dct" -l index-mode -d 'Optional index mode for this doctor run (e.g. \'lazy\', \'full\')' -r -f -a "lazy\t''
full\t''"
complete -c curd -n "__fish_curd_using_subcommand dct" -l index-scope -d 'Optional index scope for this doctor run' -r -f -a "auto\t''
forced\t''"
complete -c curd -n "__fish_curd_using_subcommand dct" -l index-max-file-size -d 'Optional max file size in bytes for this doctor run' -r
complete -c curd -n "__fish_curd_using_subcommand dct" -l index-large-file-policy -d 'Optional large file policy for this doctor run' -r -f -a "skip\t''
skeleton\t''
full\t''"
complete -c curd -n "__fish_curd_using_subcommand dct" -l index-execution -d 'Optional index execution model for this doctor run' -r -f -a "multithreaded\t''
multiprocess\t''
singlethreaded\t''"
complete -c curd -n "__fish_curd_using_subcommand dct" -l index-chunk-size -d 'Optional chunk size for multiprocess mode' -r
complete -c curd -n "__fish_curd_using_subcommand dct" -l strict -d 'Fail with non-zero status when warnings/findings are present'
complete -c curd -n "__fish_curd_using_subcommand dct" -l parity-rerun -d 'Re-run the index build twice and ensure symbol counts and hashes match exactly'
complete -c curd -n "__fish_curd_using_subcommand dct" -l compare-with-full -d 'Compare index contents against a \'full\' mode run'
complete -c curd -n "__fish_curd_using_subcommand dct" -l profile-index -d 'Run indexing multiple times to generate a performance profile'
complete -c curd -n "__fish_curd_using_subcommand dct" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand build" -l adapter -d 'Build adapter override (e.g. cargo, cmake, ninja, make, uv, poetry, pip, conda, mamba, npm, yarn, pnpm, bun)' -r
complete -c curd -n "__fish_curd_using_subcommand build" -l profile -d 'Build profile (debug|release)' -r
complete -c curd -n "__fish_curd_using_subcommand build" -l execute -d 'Execute planned commands (default: true)' -r -f -a "true\t''
false\t''"
complete -c curd -n "__fish_curd_using_subcommand build" -s c -l command -d 'Custom command to run directly, overriding adapters (e.g. `pixi run dev2`)' -r
complete -c curd -n "__fish_curd_using_subcommand build" -l plan -d 'Only show the build plan, do not execute'
complete -c curd -n "__fish_curd_using_subcommand build" -l allow-untrusted -d 'Allow execution of custom adapters defined in workspace settings.toml without prompt'
complete -c curd -n "__fish_curd_using_subcommand build" -l json -d 'Output results as JSON'
complete -c curd -n "__fish_curd_using_subcommand build" -l zig -d 'Use cargo-zigbuild instead of cargo build'
complete -c curd -n "__fish_curd_using_subcommand build" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand bld" -l adapter -d 'Build adapter override (e.g. cargo, cmake, ninja, make, uv, poetry, pip, conda, mamba, npm, yarn, pnpm, bun)' -r
complete -c curd -n "__fish_curd_using_subcommand bld" -l profile -d 'Build profile (debug|release)' -r
complete -c curd -n "__fish_curd_using_subcommand bld" -l execute -d 'Execute planned commands (default: true)' -r -f -a "true\t''
false\t''"
complete -c curd -n "__fish_curd_using_subcommand bld" -s c -l command -d 'Custom command to run directly, overriding adapters (e.g. `pixi run dev2`)' -r
complete -c curd -n "__fish_curd_using_subcommand bld" -l plan -d 'Only show the build plan, do not execute'
complete -c curd -n "__fish_curd_using_subcommand bld" -l allow-untrusted -d 'Allow execution of custom adapters defined in workspace settings.toml without prompt'
complete -c curd -n "__fish_curd_using_subcommand bld" -l json -d 'Output results as JSON'
complete -c curd -n "__fish_curd_using_subcommand bld" -l zig -d 'Use cargo-zigbuild instead of cargo build'
complete -c curd -n "__fish_curd_using_subcommand bld" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand b" -l adapter -d 'Build adapter override (e.g. cargo, cmake, ninja, make, uv, poetry, pip, conda, mamba, npm, yarn, pnpm, bun)' -r
complete -c curd -n "__fish_curd_using_subcommand b" -l profile -d 'Build profile (debug|release)' -r
complete -c curd -n "__fish_curd_using_subcommand b" -l execute -d 'Execute planned commands (default: true)' -r -f -a "true\t''
false\t''"
complete -c curd -n "__fish_curd_using_subcommand b" -s c -l command -d 'Custom command to run directly, overriding adapters (e.g. `pixi run dev2`)' -r
complete -c curd -n "__fish_curd_using_subcommand b" -l plan -d 'Only show the build plan, do not execute'
complete -c curd -n "__fish_curd_using_subcommand b" -l allow-untrusted -d 'Allow execution of custom adapters defined in workspace settings.toml without prompt'
complete -c curd -n "__fish_curd_using_subcommand b" -l json -d 'Output results as JSON'
complete -c curd -n "__fish_curd_using_subcommand b" -l zig -d 'Use cargo-zigbuild instead of cargo build'
complete -c curd -n "__fish_curd_using_subcommand b" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand hook" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand hok" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand diff" -l symbol -d 'Optional specific symbol to diff' -r
complete -c curd -n "__fish_curd_using_subcommand diff" -l semantic -d 'Semantic AST-level diff'
complete -c curd -n "__fish_curd_using_subcommand diff" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand dif" -l symbol -d 'Optional specific symbol to diff' -r
complete -c curd -n "__fish_curd_using_subcommand dif" -l semantic -d 'Semantic AST-level diff'
complete -c curd -n "__fish_curd_using_subcommand dif" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand refactor; and not __fish_seen_subcommand_from rename move extract help" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand refactor; and not __fish_seen_subcommand_from rename move extract help" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand refactor; and not __fish_seen_subcommand_from rename move extract help" -f -a "rename" -d 'Rename a function/class/variable'
complete -c curd -n "__fish_curd_using_subcommand refactor; and not __fish_seen_subcommand_from rename move extract help" -f -a "move" -d 'Move a function between files'
complete -c curd -n "__fish_curd_using_subcommand refactor; and not __fish_seen_subcommand_from rename move extract help" -f -a "extract" -d 'Extract code into a new function'
complete -c curd -n "__fish_curd_using_subcommand refactor; and not __fish_seen_subcommand_from rename move extract help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand refactor; and __fish_seen_subcommand_from rename" -l lsp -d 'Optional: Use an external LSP server for type-aware renaming (e.g., \'rust-analyzer\')' -r
complete -c curd -n "__fish_curd_using_subcommand refactor; and __fish_seen_subcommand_from rename" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand refactor; and __fish_seen_subcommand_from move" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand refactor; and __fish_seen_subcommand_from extract" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand refactor; and __fish_seen_subcommand_from help" -f -a "rename" -d 'Rename a function/class/variable'
complete -c curd -n "__fish_curd_using_subcommand refactor; and __fish_seen_subcommand_from help" -f -a "move" -d 'Move a function between files'
complete -c curd -n "__fish_curd_using_subcommand refactor; and __fish_seen_subcommand_from help" -f -a "extract" -d 'Extract code into a new function'
complete -c curd -n "__fish_curd_using_subcommand refactor; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand ref; and not __fish_seen_subcommand_from rename move extract help" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand ref; and not __fish_seen_subcommand_from rename move extract help" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ref; and not __fish_seen_subcommand_from rename move extract help" -f -a "rename" -d 'Rename a function/class/variable'
complete -c curd -n "__fish_curd_using_subcommand ref; and not __fish_seen_subcommand_from rename move extract help" -f -a "move" -d 'Move a function between files'
complete -c curd -n "__fish_curd_using_subcommand ref; and not __fish_seen_subcommand_from rename move extract help" -f -a "extract" -d 'Extract code into a new function'
complete -c curd -n "__fish_curd_using_subcommand ref; and not __fish_seen_subcommand_from rename move extract help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand ref; and __fish_seen_subcommand_from rename" -l lsp -d 'Optional: Use an external LSP server for type-aware renaming (e.g., \'rust-analyzer\')' -r
complete -c curd -n "__fish_curd_using_subcommand ref; and __fish_seen_subcommand_from rename" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ref; and __fish_seen_subcommand_from move" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ref; and __fish_seen_subcommand_from extract" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ref; and __fish_seen_subcommand_from help" -f -a "rename" -d 'Rename a function/class/variable'
complete -c curd -n "__fish_curd_using_subcommand ref; and __fish_seen_subcommand_from help" -f -a "move" -d 'Move a function between files'
complete -c curd -n "__fish_curd_using_subcommand ref; and __fish_seen_subcommand_from help" -f -a "extract" -d 'Extract code into a new function'
complete -c curd -n "__fish_curd_using_subcommand ref; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand context; and not __fish_seen_subcommand_from add remove list help" -l root -d 'Path to the primary workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand context; and not __fish_seen_subcommand_from add remove list help" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand context; and not __fish_seen_subcommand_from add remove list help" -f -a "add" -d 'Add an external repository context'
complete -c curd -n "__fish_curd_using_subcommand context; and not __fish_seen_subcommand_from add remove list help" -f -a "remove" -d 'Remove a linked context by alias or path'
complete -c curd -n "__fish_curd_using_subcommand context; and not __fish_seen_subcommand_from add remove list help" -f -a "list" -d 'List all currently linked contexts'
complete -c curd -n "__fish_curd_using_subcommand context; and not __fish_seen_subcommand_from add remove list help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand context; and __fish_seen_subcommand_from add" -l alias -d 'Optional alias for this context (auto-generated if omitted)' -r
complete -c curd -n "__fish_curd_using_subcommand context; and __fish_seen_subcommand_from add" -l index -d 'Link context in Index mode (Search and Graph only, no file reads)'
complete -c curd -n "__fish_curd_using_subcommand context; and __fish_seen_subcommand_from add" -l read -d 'Link context in Read mode (Read-only, no writes)'
complete -c curd -n "__fish_curd_using_subcommand context; and __fish_seen_subcommand_from add" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand context; and __fish_seen_subcommand_from remove" -l force -d 'Force removal even if there are dangling dependencies in the primary workspace'
complete -c curd -n "__fish_curd_using_subcommand context; and __fish_seen_subcommand_from remove" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand context; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand context; and __fish_seen_subcommand_from help" -f -a "add" -d 'Add an external repository context'
complete -c curd -n "__fish_curd_using_subcommand context; and __fish_seen_subcommand_from help" -f -a "remove" -d 'Remove a linked context by alias or path'
complete -c curd -n "__fish_curd_using_subcommand context; and __fish_seen_subcommand_from help" -f -a "list" -d 'List all currently linked contexts'
complete -c curd -n "__fish_curd_using_subcommand context; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand ctx; and not __fish_seen_subcommand_from add remove list help" -l root -d 'Path to the primary workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand ctx; and not __fish_seen_subcommand_from add remove list help" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ctx; and not __fish_seen_subcommand_from add remove list help" -f -a "add" -d 'Add an external repository context'
complete -c curd -n "__fish_curd_using_subcommand ctx; and not __fish_seen_subcommand_from add remove list help" -f -a "remove" -d 'Remove a linked context by alias or path'
complete -c curd -n "__fish_curd_using_subcommand ctx; and not __fish_seen_subcommand_from add remove list help" -f -a "list" -d 'List all currently linked contexts'
complete -c curd -n "__fish_curd_using_subcommand ctx; and not __fish_seen_subcommand_from add remove list help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand ctx; and __fish_seen_subcommand_from add" -l alias -d 'Optional alias for this context (auto-generated if omitted)' -r
complete -c curd -n "__fish_curd_using_subcommand ctx; and __fish_seen_subcommand_from add" -l index -d 'Link context in Index mode (Search and Graph only, no file reads)'
complete -c curd -n "__fish_curd_using_subcommand ctx; and __fish_seen_subcommand_from add" -l read -d 'Link context in Read mode (Read-only, no writes)'
complete -c curd -n "__fish_curd_using_subcommand ctx; and __fish_seen_subcommand_from add" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ctx; and __fish_seen_subcommand_from remove" -l force -d 'Force removal even if there are dangling dependencies in the primary workspace'
complete -c curd -n "__fish_curd_using_subcommand ctx; and __fish_seen_subcommand_from remove" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ctx; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ctx; and __fish_seen_subcommand_from help" -f -a "add" -d 'Add an external repository context'
complete -c curd -n "__fish_curd_using_subcommand ctx; and __fish_seen_subcommand_from help" -f -a "remove" -d 'Remove a linked context by alias or path'
complete -c curd -n "__fish_curd_using_subcommand ctx; and __fish_seen_subcommand_from help" -f -a "list" -d 'List all currently linked contexts'
complete -c curd -n "__fish_curd_using_subcommand ctx; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand init" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ini" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand config; and not __fish_seen_subcommand_from show set unset help" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand config; and not __fish_seen_subcommand_from show set unset help" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand config; and not __fish_seen_subcommand_from show set unset help" -f -a "show" -d 'Show current workspace configuration'
complete -c curd -n "__fish_curd_using_subcommand config; and not __fish_seen_subcommand_from show set unset help" -f -a "set" -d 'Set a configuration value (e.g. `set index.mode fast`)'
complete -c curd -n "__fish_curd_using_subcommand config; and not __fish_seen_subcommand_from show set unset help" -f -a "unset" -d 'Remove a configuration value'
complete -c curd -n "__fish_curd_using_subcommand config; and not __fish_seen_subcommand_from show set unset help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand config; and __fish_seen_subcommand_from show" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand config; and __fish_seen_subcommand_from set" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand config; and __fish_seen_subcommand_from unset" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "show" -d 'Show current workspace configuration'
complete -c curd -n "__fish_curd_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "set" -d 'Set a configuration value (e.g. `set index.mode fast`)'
complete -c curd -n "__fish_curd_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "unset" -d 'Remove a configuration value'
complete -c curd -n "__fish_curd_using_subcommand config; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand cfg; and not __fish_seen_subcommand_from show set unset help" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand cfg; and not __fish_seen_subcommand_from show set unset help" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand cfg; and not __fish_seen_subcommand_from show set unset help" -f -a "show" -d 'Show current workspace configuration'
complete -c curd -n "__fish_curd_using_subcommand cfg; and not __fish_seen_subcommand_from show set unset help" -f -a "set" -d 'Set a configuration value (e.g. `set index.mode fast`)'
complete -c curd -n "__fish_curd_using_subcommand cfg; and not __fish_seen_subcommand_from show set unset help" -f -a "unset" -d 'Remove a configuration value'
complete -c curd -n "__fish_curd_using_subcommand cfg; and not __fish_seen_subcommand_from show set unset help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand cfg; and __fish_seen_subcommand_from show" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand cfg; and __fish_seen_subcommand_from set" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand cfg; and __fish_seen_subcommand_from unset" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand cfg; and __fish_seen_subcommand_from help" -f -a "show" -d 'Show current workspace configuration'
complete -c curd -n "__fish_curd_using_subcommand cfg; and __fish_seen_subcommand_from help" -f -a "set" -d 'Set a configuration value (e.g. `set index.mode fast`)'
complete -c curd -n "__fish_curd_using_subcommand cfg; and __fish_seen_subcommand_from help" -f -a "unset" -d 'Remove a configuration value'
complete -c curd -n "__fish_curd_using_subcommand cfg; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand plugin-language; and not __fish_seen_subcommand_from list add remove help" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand plugin-language; and not __fish_seen_subcommand_from list add remove help" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plugin-language; and not __fish_seen_subcommand_from list add remove help" -f -a "list" -d 'List installed plugin packages'
complete -c curd -n "__fish_curd_using_subcommand plugin-language; and not __fish_seen_subcommand_from list add remove help" -f -a "add" -d 'Install a signed plugin archive'
complete -c curd -n "__fish_curd_using_subcommand plugin-language; and not __fish_seen_subcommand_from list add remove help" -f -a "remove" -d 'Remove an installed plugin package'
complete -c curd -n "__fish_curd_using_subcommand plugin-language; and not __fish_seen_subcommand_from list add remove help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand plugin-language; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plugin-language; and __fish_seen_subcommand_from add" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plugin-language; and __fish_seen_subcommand_from remove" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plugin-language; and __fish_seen_subcommand_from help" -f -a "list" -d 'List installed plugin packages'
complete -c curd -n "__fish_curd_using_subcommand plugin-language; and __fish_seen_subcommand_from help" -f -a "add" -d 'Install a signed plugin archive'
complete -c curd -n "__fish_curd_using_subcommand plugin-language; and __fish_seen_subcommand_from help" -f -a "remove" -d 'Remove an installed plugin package'
complete -c curd -n "__fish_curd_using_subcommand plugin-language; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand plang; and not __fish_seen_subcommand_from list add remove help" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand plang; and not __fish_seen_subcommand_from list add remove help" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plang; and not __fish_seen_subcommand_from list add remove help" -f -a "list" -d 'List installed plugin packages'
complete -c curd -n "__fish_curd_using_subcommand plang; and not __fish_seen_subcommand_from list add remove help" -f -a "add" -d 'Install a signed plugin archive'
complete -c curd -n "__fish_curd_using_subcommand plang; and not __fish_seen_subcommand_from list add remove help" -f -a "remove" -d 'Remove an installed plugin package'
complete -c curd -n "__fish_curd_using_subcommand plang; and not __fish_seen_subcommand_from list add remove help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand plang; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plang; and __fish_seen_subcommand_from add" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plang; and __fish_seen_subcommand_from remove" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plang; and __fish_seen_subcommand_from help" -f -a "list" -d 'List installed plugin packages'
complete -c curd -n "__fish_curd_using_subcommand plang; and __fish_seen_subcommand_from help" -f -a "add" -d 'Install a signed plugin archive'
complete -c curd -n "__fish_curd_using_subcommand plang; and __fish_seen_subcommand_from help" -f -a "remove" -d 'Remove an installed plugin package'
complete -c curd -n "__fish_curd_using_subcommand plang; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand plugin-tool; and not __fish_seen_subcommand_from list add remove help" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand plugin-tool; and not __fish_seen_subcommand_from list add remove help" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plugin-tool; and not __fish_seen_subcommand_from list add remove help" -f -a "list" -d 'List installed plugin packages'
complete -c curd -n "__fish_curd_using_subcommand plugin-tool; and not __fish_seen_subcommand_from list add remove help" -f -a "add" -d 'Install a signed plugin archive'
complete -c curd -n "__fish_curd_using_subcommand plugin-tool; and not __fish_seen_subcommand_from list add remove help" -f -a "remove" -d 'Remove an installed plugin package'
complete -c curd -n "__fish_curd_using_subcommand plugin-tool; and not __fish_seen_subcommand_from list add remove help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand plugin-tool; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plugin-tool; and __fish_seen_subcommand_from add" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plugin-tool; and __fish_seen_subcommand_from remove" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plugin-tool; and __fish_seen_subcommand_from help" -f -a "list" -d 'List installed plugin packages'
complete -c curd -n "__fish_curd_using_subcommand plugin-tool; and __fish_seen_subcommand_from help" -f -a "add" -d 'Install a signed plugin archive'
complete -c curd -n "__fish_curd_using_subcommand plugin-tool; and __fish_seen_subcommand_from help" -f -a "remove" -d 'Remove an installed plugin package'
complete -c curd -n "__fish_curd_using_subcommand plugin-tool; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand ptool; and not __fish_seen_subcommand_from list add remove help" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand ptool; and not __fish_seen_subcommand_from list add remove help" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ptool; and not __fish_seen_subcommand_from list add remove help" -f -a "list" -d 'List installed plugin packages'
complete -c curd -n "__fish_curd_using_subcommand ptool; and not __fish_seen_subcommand_from list add remove help" -f -a "add" -d 'Install a signed plugin archive'
complete -c curd -n "__fish_curd_using_subcommand ptool; and not __fish_seen_subcommand_from list add remove help" -f -a "remove" -d 'Remove an installed plugin package'
complete -c curd -n "__fish_curd_using_subcommand ptool; and not __fish_seen_subcommand_from list add remove help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand ptool; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ptool; and __fish_seen_subcommand_from add" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ptool; and __fish_seen_subcommand_from remove" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ptool; and __fish_seen_subcommand_from help" -f -a "list" -d 'List installed plugin packages'
complete -c curd -n "__fish_curd_using_subcommand ptool; and __fish_seen_subcommand_from help" -f -a "add" -d 'Install a signed plugin archive'
complete -c curd -n "__fish_curd_using_subcommand ptool; and __fish_seen_subcommand_from help" -f -a "remove" -d 'Remove an installed plugin package'
complete -c curd -n "__fish_curd_using_subcommand ptool; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and not __fish_seen_subcommand_from list get add remove enable disable help" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and not __fish_seen_subcommand_from list get add remove enable disable help" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and not __fish_seen_subcommand_from list get add remove enable disable help" -f -a "list" -d 'List trusted signing keys'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and not __fish_seen_subcommand_from list get add remove enable disable help" -f -a "get" -d 'Get one trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and not __fish_seen_subcommand_from list get add remove enable disable help" -f -a "add" -d 'Add or replace a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and not __fish_seen_subcommand_from list get add remove enable disable help" -f -a "remove" -d 'Remove a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and not __fish_seen_subcommand_from list get add remove enable disable help" -f -a "enable" -d 'Enable a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and not __fish_seen_subcommand_from list get add remove enable disable help" -f -a "disable" -d 'Disable a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and not __fish_seen_subcommand_from list get add remove enable disable help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and __fish_seen_subcommand_from get" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and __fish_seen_subcommand_from add" -l label -d 'Optional label' -r
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and __fish_seen_subcommand_from add" -l kind -d 'Allowed plugin kinds for this key' -r -f -a "tool\t''
language\t''"
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and __fish_seen_subcommand_from add" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and __fish_seen_subcommand_from remove" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and __fish_seen_subcommand_from enable" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and __fish_seen_subcommand_from disable" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and __fish_seen_subcommand_from help" -f -a "list" -d 'List trusted signing keys'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and __fish_seen_subcommand_from help" -f -a "get" -d 'Get one trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and __fish_seen_subcommand_from help" -f -a "add" -d 'Add or replace a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and __fish_seen_subcommand_from help" -f -a "remove" -d 'Remove a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and __fish_seen_subcommand_from help" -f -a "enable" -d 'Enable a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and __fish_seen_subcommand_from help" -f -a "disable" -d 'Disable a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand plugin-trust; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and not __fish_seen_subcommand_from list get add remove enable disable help" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand ptrust; and not __fish_seen_subcommand_from list get add remove enable disable help" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and not __fish_seen_subcommand_from list get add remove enable disable help" -f -a "list" -d 'List trusted signing keys'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and not __fish_seen_subcommand_from list get add remove enable disable help" -f -a "get" -d 'Get one trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and not __fish_seen_subcommand_from list get add remove enable disable help" -f -a "add" -d 'Add or replace a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and not __fish_seen_subcommand_from list get add remove enable disable help" -f -a "remove" -d 'Remove a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and not __fish_seen_subcommand_from list get add remove enable disable help" -f -a "enable" -d 'Enable a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and not __fish_seen_subcommand_from list get add remove enable disable help" -f -a "disable" -d 'Disable a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and not __fish_seen_subcommand_from list get add remove enable disable help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and __fish_seen_subcommand_from get" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and __fish_seen_subcommand_from add" -l label -d 'Optional label' -r
complete -c curd -n "__fish_curd_using_subcommand ptrust; and __fish_seen_subcommand_from add" -l kind -d 'Allowed plugin kinds for this key' -r -f -a "tool\t''
language\t''"
complete -c curd -n "__fish_curd_using_subcommand ptrust; and __fish_seen_subcommand_from add" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and __fish_seen_subcommand_from remove" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and __fish_seen_subcommand_from enable" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and __fish_seen_subcommand_from disable" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and __fish_seen_subcommand_from help" -f -a "list" -d 'List trusted signing keys'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and __fish_seen_subcommand_from help" -f -a "get" -d 'Get one trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and __fish_seen_subcommand_from help" -f -a "add" -d 'Add or replace a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and __fish_seen_subcommand_from help" -f -a "remove" -d 'Remove a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and __fish_seen_subcommand_from help" -f -a "enable" -d 'Enable a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and __fish_seen_subcommand_from help" -f -a "disable" -d 'Disable a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand ptrust; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand detach" -l shadow -d 'How to handle an active shadow transaction before detaching' -r -f -a "apply\t''
discard\t''
abort\t''"
complete -c curd -n "__fish_curd_using_subcommand detach" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand det" -l shadow -d 'How to handle an active shadow transaction before detaching' -r -f -a "apply\t''
discard\t''
abort\t''"
complete -c curd -n "__fish_curd_using_subcommand det" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand delete" -l shadow -d 'How to handle an active shadow transaction before deleting CURD state' -r -f -a "apply\t''
discard\t''
abort\t''"
complete -c curd -n "__fish_curd_using_subcommand delete" -s y -l yes -d 'Force skip confirmation'
complete -c curd -n "__fish_curd_using_subcommand delete" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand del" -l shadow -d 'How to handle an active shadow transaction before deleting CURD state' -r -f -a "apply\t''
discard\t''
abort\t''"
complete -c curd -n "__fish_curd_using_subcommand del" -s y -l yes -d 'Force skip confirmation'
complete -c curd -n "__fish_curd_using_subcommand del" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand status" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand st" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand sts" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand log" -s l -l limit -d 'Number of recent entries to show' -r
complete -c curd -n "__fish_curd_using_subcommand log" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand repl" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand rpl" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand run" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand run" -l arg -d 'Script argument override in key=value form' -r
complete -c curd -n "__fish_curd_using_subcommand run" -l profile -d 'Optional profile override' -r
complete -c curd -n "__fish_curd_using_subcommand run" -l out -d 'Output path for `curd run compile ...`' -r -F
complete -c curd -n "__fish_curd_using_subcommand run" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand test" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand test" -s v -l verbose -d 'Show detailed list of broken links and dead zones'
complete -c curd -n "__fish_curd_using_subcommand test" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand tst" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand tst" -s v -l verbose -d 'Show detailed list of broken links and dead zones'
complete -c curd -n "__fish_curd_using_subcommand tst" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand search" -l kind -d 'Optional kind filter (e.g. \'function\', \'class\')' -r
complete -c curd -n "__fish_curd_using_subcommand search" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand search" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand sch" -l kind -d 'Optional kind filter (e.g. \'function\', \'class\')' -r
complete -c curd -n "__fish_curd_using_subcommand sch" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand sch" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand s" -l kind -d 'Optional kind filter (e.g. \'function\', \'class\')' -r
complete -c curd -n "__fish_curd_using_subcommand s" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand s" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand graph" -l depth -d 'The traversal depth' -r
complete -c curd -n "__fish_curd_using_subcommand graph" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand graph" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand g" -l depth -d 'The traversal depth' -r
complete -c curd -n "__fish_curd_using_subcommand g" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand g" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand read" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand read" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand red" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand red" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand edit" -s a -l action -d 'The action to perform (\'upsert\' or \'delete\')' -r
complete -c curd -n "__fish_curd_using_subcommand edit" -s c -l code -d 'The literal code to insert (ignored if action is \'delete\')' -r
complete -c curd -n "__fish_curd_using_subcommand edit" -l base-state-hash -d 'Optional: The expected Merkle Root of the workspace state for optimistic concurrency control' -r
complete -c curd -n "__fish_curd_using_subcommand edit" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand edit" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand edt" -s a -l action -d 'The action to perform (\'upsert\' or \'delete\')' -r
complete -c curd -n "__fish_curd_using_subcommand edt" -s c -l code -d 'The literal code to insert (ignored if action is \'delete\')' -r
complete -c curd -n "__fish_curd_using_subcommand edt" -l base-state-hash -d 'Optional: The expected Merkle Root of the workspace state for optimistic concurrency control' -r
complete -c curd -n "__fish_curd_using_subcommand edt" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand edt" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand e" -s a -l action -d 'The action to perform (\'upsert\' or \'delete\')' -r
complete -c curd -n "__fish_curd_using_subcommand e" -s c -l code -d 'The literal code to insert (ignored if action is \'delete\')' -r
complete -c curd -n "__fish_curd_using_subcommand e" -l base-state-hash -d 'Optional: The expected Merkle Root of the workspace state for optimistic concurrency control' -r
complete -c curd -n "__fish_curd_using_subcommand e" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand e" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand diagram" -s u -l uris -d 'Symbol URIs to include as roots' -r
complete -c curd -n "__fish_curd_using_subcommand diagram" -s r -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand diagram" -s f -l format -d 'Output format (ascii, svg, dot, mermaid)' -r
complete -c curd -n "__fish_curd_using_subcommand diagram" -l depth -d 'Traversal depth for dependencies' -r
complete -c curd -n "__fish_curd_using_subcommand diagram" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand dia" -s u -l uris -d 'Symbol URIs to include as roots' -r
complete -c curd -n "__fish_curd_using_subcommand dia" -s r -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand dia" -s f -l format -d 'Output format (ascii, svg, dot, mermaid)' -r
complete -c curd -n "__fish_curd_using_subcommand dia" -l depth -d 'Traversal depth for dependencies' -r
complete -c curd -n "__fish_curd_using_subcommand dia" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand session; and not __fish_seen_subcommand_from begin review log commit rollback help" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand session; and not __fish_seen_subcommand_from begin review log commit rollback help" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand session; and not __fish_seen_subcommand_from begin review log commit rollback help" -f -a "begin" -d 'Open a new shadow transaction'
complete -c curd -n "__fish_curd_using_subcommand session; and not __fish_seen_subcommand_from begin review log commit rollback help" -f -a "review" -d 'View changes and architectural impact of the current session'
complete -c curd -n "__fish_curd_using_subcommand session; and not __fish_seen_subcommand_from begin review log commit rollback help" -f -a "log" -d 'View a detailed log of all tool calls and their results'
complete -c curd -n "__fish_curd_using_subcommand session; and not __fish_seen_subcommand_from begin review log commit rollback help" -f -a "commit" -d 'Commit the active shadow transaction to disk'
complete -c curd -n "__fish_curd_using_subcommand session; and not __fish_seen_subcommand_from begin review log commit rollback help" -f -a "rollback" -d 'Discard the active shadow transaction'
complete -c curd -n "__fish_curd_using_subcommand session; and not __fish_seen_subcommand_from begin review log commit rollback help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand session; and __fish_seen_subcommand_from begin" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand session; and __fish_seen_subcommand_from review" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand session; and __fish_seen_subcommand_from log" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand session; and __fish_seen_subcommand_from commit" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand session; and __fish_seen_subcommand_from rollback" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand session; and __fish_seen_subcommand_from help" -f -a "begin" -d 'Open a new shadow transaction'
complete -c curd -n "__fish_curd_using_subcommand session; and __fish_seen_subcommand_from help" -f -a "review" -d 'View changes and architectural impact of the current session'
complete -c curd -n "__fish_curd_using_subcommand session; and __fish_seen_subcommand_from help" -f -a "log" -d 'View a detailed log of all tool calls and their results'
complete -c curd -n "__fish_curd_using_subcommand session; and __fish_seen_subcommand_from help" -f -a "commit" -d 'Commit the active shadow transaction to disk'
complete -c curd -n "__fish_curd_using_subcommand session; and __fish_seen_subcommand_from help" -f -a "rollback" -d 'Discard the active shadow transaction'
complete -c curd -n "__fish_curd_using_subcommand session; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand ses; and not __fish_seen_subcommand_from begin review log commit rollback help" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand ses; and not __fish_seen_subcommand_from begin review log commit rollback help" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ses; and not __fish_seen_subcommand_from begin review log commit rollback help" -f -a "begin" -d 'Open a new shadow transaction'
complete -c curd -n "__fish_curd_using_subcommand ses; and not __fish_seen_subcommand_from begin review log commit rollback help" -f -a "review" -d 'View changes and architectural impact of the current session'
complete -c curd -n "__fish_curd_using_subcommand ses; and not __fish_seen_subcommand_from begin review log commit rollback help" -f -a "log" -d 'View a detailed log of all tool calls and their results'
complete -c curd -n "__fish_curd_using_subcommand ses; and not __fish_seen_subcommand_from begin review log commit rollback help" -f -a "commit" -d 'Commit the active shadow transaction to disk'
complete -c curd -n "__fish_curd_using_subcommand ses; and not __fish_seen_subcommand_from begin review log commit rollback help" -f -a "rollback" -d 'Discard the active shadow transaction'
complete -c curd -n "__fish_curd_using_subcommand ses; and not __fish_seen_subcommand_from begin review log commit rollback help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand ses; and __fish_seen_subcommand_from begin" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ses; and __fish_seen_subcommand_from review" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ses; and __fish_seen_subcommand_from log" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ses; and __fish_seen_subcommand_from commit" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ses; and __fish_seen_subcommand_from rollback" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand ses; and __fish_seen_subcommand_from help" -f -a "begin" -d 'Open a new shadow transaction'
complete -c curd -n "__fish_curd_using_subcommand ses; and __fish_seen_subcommand_from help" -f -a "review" -d 'View changes and architectural impact of the current session'
complete -c curd -n "__fish_curd_using_subcommand ses; and __fish_seen_subcommand_from help" -f -a "log" -d 'View a detailed log of all tool calls and their results'
complete -c curd -n "__fish_curd_using_subcommand ses; and __fish_seen_subcommand_from help" -f -a "commit" -d 'Commit the active shadow transaction to disk'
complete -c curd -n "__fish_curd_using_subcommand ses; and __fish_seen_subcommand_from help" -f -a "rollback" -d 'Discard the active shadow transaction'
complete -c curd -n "__fish_curd_using_subcommand ses; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand plan; and not __fish_seen_subcommand_from list read edit impl help" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand plan; and not __fish_seen_subcommand_from list read edit impl help" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plan; and not __fish_seen_subcommand_from list read edit impl help" -f -a "list" -d 'List all saved plans in the workspace'
complete -c curd -n "__fish_curd_using_subcommand plan; and not __fish_seen_subcommand_from list read edit impl help" -f -a "read" -d 'Read a specific plan and format it as a readable execution tree'
complete -c curd -n "__fish_curd_using_subcommand plan; and not __fish_seen_subcommand_from list read edit impl help" -f -a "edit" -d 'Edit a saved plan or compiled script artifact interactively'
complete -c curd -n "__fish_curd_using_subcommand plan; and not __fish_seen_subcommand_from list read edit impl help" -f -a "impl" -d 'Execute a saved plan'
complete -c curd -n "__fish_curd_using_subcommand plan; and not __fish_seen_subcommand_from list read edit impl help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand plan; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plan; and __fish_seen_subcommand_from read" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plan; and __fish_seen_subcommand_from edit" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plan; and __fish_seen_subcommand_from impl" -l session -d 'The UUID of the active session this plan belongs to' -r
complete -c curd -n "__fish_curd_using_subcommand plan; and __fish_seen_subcommand_from impl" -l dry-run -d 'Perform a dry-run validation (simulate) instead of executing'
complete -c curd -n "__fish_curd_using_subcommand plan; and __fish_seen_subcommand_from impl" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand plan; and __fish_seen_subcommand_from help" -f -a "list" -d 'List all saved plans in the workspace'
complete -c curd -n "__fish_curd_using_subcommand plan; and __fish_seen_subcommand_from help" -f -a "read" -d 'Read a specific plan and format it as a readable execution tree'
complete -c curd -n "__fish_curd_using_subcommand plan; and __fish_seen_subcommand_from help" -f -a "edit" -d 'Edit a saved plan or compiled script artifact interactively'
complete -c curd -n "__fish_curd_using_subcommand plan; and __fish_seen_subcommand_from help" -f -a "impl" -d 'Execute a saved plan'
complete -c curd -n "__fish_curd_using_subcommand plan; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand pln; and not __fish_seen_subcommand_from list read edit impl help" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand pln; and not __fish_seen_subcommand_from list read edit impl help" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand pln; and not __fish_seen_subcommand_from list read edit impl help" -f -a "list" -d 'List all saved plans in the workspace'
complete -c curd -n "__fish_curd_using_subcommand pln; and not __fish_seen_subcommand_from list read edit impl help" -f -a "read" -d 'Read a specific plan and format it as a readable execution tree'
complete -c curd -n "__fish_curd_using_subcommand pln; and not __fish_seen_subcommand_from list read edit impl help" -f -a "edit" -d 'Edit a saved plan or compiled script artifact interactively'
complete -c curd -n "__fish_curd_using_subcommand pln; and not __fish_seen_subcommand_from list read edit impl help" -f -a "impl" -d 'Execute a saved plan'
complete -c curd -n "__fish_curd_using_subcommand pln; and not __fish_seen_subcommand_from list read edit impl help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand pln; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand pln; and __fish_seen_subcommand_from read" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand pln; and __fish_seen_subcommand_from edit" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand pln; and __fish_seen_subcommand_from impl" -l session -d 'The UUID of the active session this plan belongs to' -r
complete -c curd -n "__fish_curd_using_subcommand pln; and __fish_seen_subcommand_from impl" -l dry-run -d 'Perform a dry-run validation (simulate) instead of executing'
complete -c curd -n "__fish_curd_using_subcommand pln; and __fish_seen_subcommand_from impl" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand pln; and __fish_seen_subcommand_from help" -f -a "list" -d 'List all saved plans in the workspace'
complete -c curd -n "__fish_curd_using_subcommand pln; and __fish_seen_subcommand_from help" -f -a "read" -d 'Read a specific plan and format it as a readable execution tree'
complete -c curd -n "__fish_curd_using_subcommand pln; and __fish_seen_subcommand_from help" -f -a "edit" -d 'Edit a saved plan or compiled script artifact interactively'
complete -c curd -n "__fish_curd_using_subcommand pln; and __fish_seen_subcommand_from help" -f -a "impl" -d 'Execute a saved plan'
complete -c curd -n "__fish_curd_using_subcommand pln; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand p; and not __fish_seen_subcommand_from list read edit impl help" -l root -d 'Path to the workspace root' -r -F
complete -c curd -n "__fish_curd_using_subcommand p; and not __fish_seen_subcommand_from list read edit impl help" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand p; and not __fish_seen_subcommand_from list read edit impl help" -f -a "list" -d 'List all saved plans in the workspace'
complete -c curd -n "__fish_curd_using_subcommand p; and not __fish_seen_subcommand_from list read edit impl help" -f -a "read" -d 'Read a specific plan and format it as a readable execution tree'
complete -c curd -n "__fish_curd_using_subcommand p; and not __fish_seen_subcommand_from list read edit impl help" -f -a "edit" -d 'Edit a saved plan or compiled script artifact interactively'
complete -c curd -n "__fish_curd_using_subcommand p; and not __fish_seen_subcommand_from list read edit impl help" -f -a "impl" -d 'Execute a saved plan'
complete -c curd -n "__fish_curd_using_subcommand p; and not __fish_seen_subcommand_from list read edit impl help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand p; and __fish_seen_subcommand_from list" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand p; and __fish_seen_subcommand_from read" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand p; and __fish_seen_subcommand_from edit" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand p; and __fish_seen_subcommand_from impl" -l session -d 'The UUID of the active session this plan belongs to' -r
complete -c curd -n "__fish_curd_using_subcommand p; and __fish_seen_subcommand_from impl" -l dry-run -d 'Perform a dry-run validation (simulate) instead of executing'
complete -c curd -n "__fish_curd_using_subcommand p; and __fish_seen_subcommand_from impl" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand p; and __fish_seen_subcommand_from help" -f -a "list" -d 'List all saved plans in the workspace'
complete -c curd -n "__fish_curd_using_subcommand p; and __fish_seen_subcommand_from help" -f -a "read" -d 'Read a specific plan and format it as a readable execution tree'
complete -c curd -n "__fish_curd_using_subcommand p; and __fish_seen_subcommand_from help" -f -a "edit" -d 'Edit a saved plan or compiled script artifact interactively'
complete -c curd -n "__fish_curd_using_subcommand p; and __fish_seen_subcommand_from help" -f -a "impl" -d 'Execute a saved plan'
complete -c curd -n "__fish_curd_using_subcommand p; and __fish_seen_subcommand_from help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand index-worker" -l request -r -F
complete -c curd -n "__fish_curd_using_subcommand index-worker" -l response -r -F
complete -c curd -n "__fish_curd_using_subcommand index-worker" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand completions" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand man" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand index" -l max-total-ms -d 'Optional threshold: fail when index total_ms exceeds this value' -r
complete -c curd -n "__fish_curd_using_subcommand index" -l max-parse-fail -d 'Optional threshold: fail when parse_fail count exceeds this value' -r
complete -c curd -n "__fish_curd_using_subcommand index" -l max-no-symbols-ratio -d 'Optional threshold: fail when no_symbols/total_files exceeds this ratio (0.0..1.0)' -r
complete -c curd -n "__fish_curd_using_subcommand index" -l max-skipped-large-ratio -d 'Optional threshold: fail when skipped_large/total_files exceeds this ratio (0.0..1.0)' -r
complete -c curd -n "__fish_curd_using_subcommand index" -l min-coverage-ratio -d 'Optional threshold: fail when coverage_ratio is below this value (0.0..1.0)' -r
complete -c curd -n "__fish_curd_using_subcommand index" -l require-coverage-state -d 'Optional threshold: fail when coverage state does not match this exactly (e.g. \'full\')' -r
complete -c curd -n "__fish_curd_using_subcommand index" -l min-symbol-count -d 'Optional threshold: fail when total symbols found is below this count' -r
complete -c curd -n "__fish_curd_using_subcommand index" -l min-symbols-per-k-files -d 'Optional threshold: fail when symbol density is below this value' -r
complete -c curd -n "__fish_curd_using_subcommand index" -l min-overlap-with-full -d 'Optional threshold: fail when overlap with full index is below this value' -r
complete -c curd -n "__fish_curd_using_subcommand index" -l report-out -d 'Write report as JSON to this path' -r -F
complete -c curd -n "__fish_curd_using_subcommand index" -l profile -d 'The profile of thresholds to apply (\'ci_fast\' or \'ci_strict\'). Overrides config' -r
complete -c curd -n "__fish_curd_using_subcommand index" -l index-mode -d 'Optional index mode for this doctor run (e.g. \'lazy\', \'full\')' -r -f -a "lazy\t''
full\t''"
complete -c curd -n "__fish_curd_using_subcommand index" -l index-scope -d 'Optional index scope for this doctor run' -r -f -a "auto\t''
forced\t''"
complete -c curd -n "__fish_curd_using_subcommand index" -l index-max-file-size -d 'Optional max file size in bytes for this doctor run' -r
complete -c curd -n "__fish_curd_using_subcommand index" -l index-large-file-policy -d 'Optional large file policy for this doctor run' -r -f -a "skip\t''
skeleton\t''
full\t''"
complete -c curd -n "__fish_curd_using_subcommand index" -l index-execution -d 'Optional index execution model for this doctor run' -r -f -a "multithreaded\t''
multiprocess\t''
singlethreaded\t''"
complete -c curd -n "__fish_curd_using_subcommand index" -l index-chunk-size -d 'Optional chunk size for multiprocess mode' -r
complete -c curd -n "__fish_curd_using_subcommand index" -l strict -d 'Fail with non-zero status when warnings/findings are present'
complete -c curd -n "__fish_curd_using_subcommand index" -l parity-rerun -d 'Re-run the index build twice and ensure symbol counts and hashes match exactly'
complete -c curd -n "__fish_curd_using_subcommand index" -l compare-with-full -d 'Compare index contents against a \'full\' mode run'
complete -c curd -n "__fish_curd_using_subcommand index" -l profile-index -d 'Run indexing multiple times to generate a performance profile'
complete -c curd -n "__fish_curd_using_subcommand index" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand idx" -l max-total-ms -d 'Optional threshold: fail when index total_ms exceeds this value' -r
complete -c curd -n "__fish_curd_using_subcommand idx" -l max-parse-fail -d 'Optional threshold: fail when parse_fail count exceeds this value' -r
complete -c curd -n "__fish_curd_using_subcommand idx" -l max-no-symbols-ratio -d 'Optional threshold: fail when no_symbols/total_files exceeds this ratio (0.0..1.0)' -r
complete -c curd -n "__fish_curd_using_subcommand idx" -l max-skipped-large-ratio -d 'Optional threshold: fail when skipped_large/total_files exceeds this ratio (0.0..1.0)' -r
complete -c curd -n "__fish_curd_using_subcommand idx" -l min-coverage-ratio -d 'Optional threshold: fail when coverage_ratio is below this value (0.0..1.0)' -r
complete -c curd -n "__fish_curd_using_subcommand idx" -l require-coverage-state -d 'Optional threshold: fail when coverage state does not match this exactly (e.g. \'full\')' -r
complete -c curd -n "__fish_curd_using_subcommand idx" -l min-symbol-count -d 'Optional threshold: fail when total symbols found is below this count' -r
complete -c curd -n "__fish_curd_using_subcommand idx" -l min-symbols-per-k-files -d 'Optional threshold: fail when symbol density is below this value' -r
complete -c curd -n "__fish_curd_using_subcommand idx" -l min-overlap-with-full -d 'Optional threshold: fail when overlap with full index is below this value' -r
complete -c curd -n "__fish_curd_using_subcommand idx" -l report-out -d 'Write report as JSON to this path' -r -F
complete -c curd -n "__fish_curd_using_subcommand idx" -l profile -d 'The profile of thresholds to apply (\'ci_fast\' or \'ci_strict\'). Overrides config' -r
complete -c curd -n "__fish_curd_using_subcommand idx" -l index-mode -d 'Optional index mode for this doctor run (e.g. \'lazy\', \'full\')' -r -f -a "lazy\t''
full\t''"
complete -c curd -n "__fish_curd_using_subcommand idx" -l index-scope -d 'Optional index scope for this doctor run' -r -f -a "auto\t''
forced\t''"
complete -c curd -n "__fish_curd_using_subcommand idx" -l index-max-file-size -d 'Optional max file size in bytes for this doctor run' -r
complete -c curd -n "__fish_curd_using_subcommand idx" -l index-large-file-policy -d 'Optional large file policy for this doctor run' -r -f -a "skip\t''
skeleton\t''
full\t''"
complete -c curd -n "__fish_curd_using_subcommand idx" -l index-execution -d 'Optional index execution model for this doctor run' -r -f -a "multithreaded\t''
multiprocess\t''
singlethreaded\t''"
complete -c curd -n "__fish_curd_using_subcommand idx" -l index-chunk-size -d 'Optional chunk size for multiprocess mode' -r
complete -c curd -n "__fish_curd_using_subcommand idx" -l strict -d 'Fail with non-zero status when warnings/findings are present'
complete -c curd -n "__fish_curd_using_subcommand idx" -l parity-rerun -d 'Re-run the index build twice and ensure symbol counts and hashes match exactly'
complete -c curd -n "__fish_curd_using_subcommand idx" -l compare-with-full -d 'Compare index contents against a \'full\' mode run'
complete -c curd -n "__fish_curd_using_subcommand idx" -l profile-index -d 'Run indexing multiple times to generate a performance profile'
complete -c curd -n "__fish_curd_using_subcommand idx" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand h" -s h -l help -d 'Print help'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "mcp" -d 'Start the Model Context Protocol (MCP) server over stdin/stdout'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "init-agent" -d 'Initialize and authorize a new agent keypair, auto-configuring a specified harness'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "doctor" -d 'Run built-in self diagnostics for indexing and regressions'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "build" -d 'Build via CURD control plane (adapter-based dry-run/execute)'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "hook" -d 'Print a shell hook to implicitly route standard commands (like \'make\') through CURD'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "diff" -d 'Compare symbols at AST level'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "refactor" -d 'Semantic refactoring engine'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "context" -d 'Manage external workspace contexts (Read-Only or Semantic linking)'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "init" -d 'Initialize CURD workspace: auto-detect build system, create .curd/ directory'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "config" -d 'Manage CURD configuration and policies'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "plugin-language" -d 'Install, remove, or list signed language plugins (.curdl)'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "plugin-tool" -d 'Install, remove, or list signed tool plugins (.curdt)'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "plugin-trust" -d 'Manage trusted signing keys for CURD plugin packages'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "detach" -d 'Soft detach CURD from the current workspace (removes git hooks and scrubs scripts)'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "delete" -d 'Permanently delete CURD from the current workspace by removing the .curd/ directory'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "status" -d 'Print a summary of the current workspace state (index stats, shadow changes, etc.)'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "log" -d 'Tail the agent\'s mutation and execution history'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "repl" -d 'Start the interactive CURD REPL for semantic exploration'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "run" -d 'Run a .curd script by compiling it to the current DSL IR'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "test" -d 'Run semantic integrity tests on the symbol graph'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "search" -d 'Semantic search across the indexed graph'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "graph" -d 'Explore the caller/callee dependency graph of a symbol'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "read" -d 'Read a file or semantic symbol exactly as the engine parses it'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "edit" -d 'Manually mutate a file or symbol via the AST-aware EditEngine'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "diagram" -d 'Generate diagrams from the symbol graph'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "session" -d 'Manage manual ShadowStore transaction sessions'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "plan" -d 'Manage, inspect, and execute saved AI plans'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "index-worker"
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "completions" -d 'Generate shell completion scripts'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "man" -d 'Generate man pages'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "index" -d 'Alias for curd doctor indexing'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "h" -d 'Show help for CURD'
complete -c curd -n "__fish_curd_using_subcommand help; and not __fish_seen_subcommand_from mcp init-agent doctor build hook diff refactor context init config plugin-language plugin-tool plugin-trust detach delete status log repl run test search graph read edit diagram session plan index-worker completions man index h help" -f -a "help" -d 'Print this message or the help of the given subcommand(s)'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from refactor" -f -a "rename" -d 'Rename a function/class/variable'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from refactor" -f -a "move" -d 'Move a function between files'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from refactor" -f -a "extract" -d 'Extract code into a new function'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from context" -f -a "add" -d 'Add an external repository context'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from context" -f -a "remove" -d 'Remove a linked context by alias or path'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from context" -f -a "list" -d 'List all currently linked contexts'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "show" -d 'Show current workspace configuration'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "set" -d 'Set a configuration value (e.g. `set index.mode fast`)'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from config" -f -a "unset" -d 'Remove a configuration value'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from plugin-language" -f -a "list" -d 'List installed plugin packages'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from plugin-language" -f -a "add" -d 'Install a signed plugin archive'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from plugin-language" -f -a "remove" -d 'Remove an installed plugin package'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from plugin-tool" -f -a "list" -d 'List installed plugin packages'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from plugin-tool" -f -a "add" -d 'Install a signed plugin archive'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from plugin-tool" -f -a "remove" -d 'Remove an installed plugin package'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from plugin-trust" -f -a "list" -d 'List trusted signing keys'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from plugin-trust" -f -a "get" -d 'Get one trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from plugin-trust" -f -a "add" -d 'Add or replace a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from plugin-trust" -f -a "remove" -d 'Remove a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from plugin-trust" -f -a "enable" -d 'Enable a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from plugin-trust" -f -a "disable" -d 'Disable a trusted signing key'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from session" -f -a "begin" -d 'Open a new shadow transaction'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from session" -f -a "review" -d 'View changes and architectural impact of the current session'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from session" -f -a "log" -d 'View a detailed log of all tool calls and their results'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from session" -f -a "commit" -d 'Commit the active shadow transaction to disk'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from session" -f -a "rollback" -d 'Discard the active shadow transaction'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from plan" -f -a "list" -d 'List all saved plans in the workspace'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from plan" -f -a "read" -d 'Read a specific plan and format it as a readable execution tree'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from plan" -f -a "edit" -d 'Edit a saved plan or compiled script artifact interactively'
complete -c curd -n "__fish_curd_using_subcommand help; and __fish_seen_subcommand_from plan" -f -a "impl" -d 'Execute a saved plan'

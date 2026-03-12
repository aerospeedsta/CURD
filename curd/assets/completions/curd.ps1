
using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName 'curd' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandElements = $commandAst.CommandElements
    $command = @(
        'curd'
        for ($i = 1; $i -lt $commandElements.Count; $i++) {
            $element = $commandElements[$i]
            if ($element -isnot [StringConstantExpressionAst] -or
                $element.StringConstantType -ne [StringConstantType]::BareWord -or
                $element.Value.StartsWith('-') -or
                $element.Value -eq $wordToComplete) {
                break
        }
        $element.Value
    }) -join ';'

    $completions = @(switch ($command) {
        'curd' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, 'Print version')
            [CompletionResult]::new('mcp', 'mcp', [CompletionResultType]::ParameterValue, 'Start the Model Context Protocol (MCP) server over stdin/stdout')
            [CompletionResult]::new('init-agent', 'init-agent', [CompletionResultType]::ParameterValue, 'Initialize and authorize a new agent keypair, auto-configuring a specified harness')
            [CompletionResult]::new('ina', 'ina', [CompletionResultType]::ParameterValue, 'Initialize and authorize a new agent keypair, auto-configuring a specified harness')
            [CompletionResult]::new('agt', 'agt', [CompletionResultType]::ParameterValue, 'Initialize and authorize a new agent keypair, auto-configuring a specified harness')
            [CompletionResult]::new('doctor', 'doctor', [CompletionResultType]::ParameterValue, 'Run built-in self diagnostics for indexing and regressions')
            [CompletionResult]::new('dct', 'dct', [CompletionResultType]::ParameterValue, 'Run built-in self diagnostics for indexing and regressions')
            [CompletionResult]::new('build', 'build', [CompletionResultType]::ParameterValue, 'Build via CURD control plane (adapter-based dry-run/execute)')
            [CompletionResult]::new('bld', 'bld', [CompletionResultType]::ParameterValue, 'Build via CURD control plane (adapter-based dry-run/execute)')
            [CompletionResult]::new('b', 'b', [CompletionResultType]::ParameterValue, 'Build via CURD control plane (adapter-based dry-run/execute)')
            [CompletionResult]::new('hook', 'hook', [CompletionResultType]::ParameterValue, 'Print a shell hook to implicitly route standard commands (like ''make'') through CURD')
            [CompletionResult]::new('hok', 'hok', [CompletionResultType]::ParameterValue, 'Print a shell hook to implicitly route standard commands (like ''make'') through CURD')
            [CompletionResult]::new('diff', 'diff', [CompletionResultType]::ParameterValue, 'Compare symbols at AST level')
            [CompletionResult]::new('dif', 'dif', [CompletionResultType]::ParameterValue, 'Compare symbols at AST level')
            [CompletionResult]::new('refactor', 'refactor', [CompletionResultType]::ParameterValue, 'Semantic refactoring engine')
            [CompletionResult]::new('ref', 'ref', [CompletionResultType]::ParameterValue, 'Semantic refactoring engine')
            [CompletionResult]::new('context', 'context', [CompletionResultType]::ParameterValue, 'Manage external workspace contexts (Read-Only or Semantic linking)')
            [CompletionResult]::new('ctx', 'ctx', [CompletionResultType]::ParameterValue, 'Manage external workspace contexts (Read-Only or Semantic linking)')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Initialize CURD workspace: auto-detect build system, create .curd/ directory')
            [CompletionResult]::new('ini', 'ini', [CompletionResultType]::ParameterValue, 'Initialize CURD workspace: auto-detect build system, create .curd/ directory')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'Manage CURD configuration and policies')
            [CompletionResult]::new('cfg', 'cfg', [CompletionResultType]::ParameterValue, 'Manage CURD configuration and policies')
            [CompletionResult]::new('plugin-language', 'plugin-language', [CompletionResultType]::ParameterValue, 'Install, remove, or list signed language plugins (.curdl)')
            [CompletionResult]::new('plang', 'plang', [CompletionResultType]::ParameterValue, 'Install, remove, or list signed language plugins (.curdl)')
            [CompletionResult]::new('plugin-tool', 'plugin-tool', [CompletionResultType]::ParameterValue, 'Install, remove, or list signed tool plugins (.curdt)')
            [CompletionResult]::new('ptool', 'ptool', [CompletionResultType]::ParameterValue, 'Install, remove, or list signed tool plugins (.curdt)')
            [CompletionResult]::new('plugin-trust', 'plugin-trust', [CompletionResultType]::ParameterValue, 'Manage trusted signing keys for CURD plugin packages')
            [CompletionResult]::new('ptrust', 'ptrust', [CompletionResultType]::ParameterValue, 'Manage trusted signing keys for CURD plugin packages')
            [CompletionResult]::new('detach', 'detach', [CompletionResultType]::ParameterValue, 'Soft detach CURD from the current workspace (removes git hooks and scrubs scripts)')
            [CompletionResult]::new('det', 'det', [CompletionResultType]::ParameterValue, 'Soft detach CURD from the current workspace (removes git hooks and scrubs scripts)')
            [CompletionResult]::new('delete', 'delete', [CompletionResultType]::ParameterValue, 'Permanently delete CURD from the current workspace by removing the .curd/ directory')
            [CompletionResult]::new('del', 'del', [CompletionResultType]::ParameterValue, 'Permanently delete CURD from the current workspace by removing the .curd/ directory')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Print a summary of the current workspace state (index stats, shadow changes, etc.)')
            [CompletionResult]::new('st', 'st', [CompletionResultType]::ParameterValue, 'Print a summary of the current workspace state (index stats, shadow changes, etc.)')
            [CompletionResult]::new('sts', 'sts', [CompletionResultType]::ParameterValue, 'Print a summary of the current workspace state (index stats, shadow changes, etc.)')
            [CompletionResult]::new('log', 'log', [CompletionResultType]::ParameterValue, 'Tail the agent''s mutation and execution history')
            [CompletionResult]::new('repl', 'repl', [CompletionResultType]::ParameterValue, 'Start the interactive CURD REPL for semantic exploration')
            [CompletionResult]::new('rpl', 'rpl', [CompletionResultType]::ParameterValue, 'Start the interactive CURD REPL for semantic exploration')
            [CompletionResult]::new('run', 'run', [CompletionResultType]::ParameterValue, 'Run a .curd script by compiling it to the current DSL IR')
            [CompletionResult]::new('test', 'test', [CompletionResultType]::ParameterValue, 'Run semantic integrity tests on the symbol graph')
            [CompletionResult]::new('tst', 'tst', [CompletionResultType]::ParameterValue, 'Run semantic integrity tests on the symbol graph')
            [CompletionResult]::new('search', 'search', [CompletionResultType]::ParameterValue, 'Semantic search across the indexed graph')
            [CompletionResult]::new('sch', 'sch', [CompletionResultType]::ParameterValue, 'Semantic search across the indexed graph')
            [CompletionResult]::new('s', 's', [CompletionResultType]::ParameterValue, 'Semantic search across the indexed graph')
            [CompletionResult]::new('graph', 'graph', [CompletionResultType]::ParameterValue, 'Explore the caller/callee dependency graph of a symbol')
            [CompletionResult]::new('g', 'g', [CompletionResultType]::ParameterValue, 'Explore the caller/callee dependency graph of a symbol')
            [CompletionResult]::new('read', 'read', [CompletionResultType]::ParameterValue, 'Read a file or semantic symbol exactly as the engine parses it')
            [CompletionResult]::new('red', 'red', [CompletionResultType]::ParameterValue, 'Read a file or semantic symbol exactly as the engine parses it')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Manually mutate a file or symbol via the AST-aware EditEngine')
            [CompletionResult]::new('edt', 'edt', [CompletionResultType]::ParameterValue, 'Manually mutate a file or symbol via the AST-aware EditEngine')
            [CompletionResult]::new('e', 'e', [CompletionResultType]::ParameterValue, 'Manually mutate a file or symbol via the AST-aware EditEngine')
            [CompletionResult]::new('diagram', 'diagram', [CompletionResultType]::ParameterValue, 'Generate diagrams from the symbol graph')
            [CompletionResult]::new('dia', 'dia', [CompletionResultType]::ParameterValue, 'Generate diagrams from the symbol graph')
            [CompletionResult]::new('session', 'session', [CompletionResultType]::ParameterValue, 'Manage manual ShadowStore transaction sessions')
            [CompletionResult]::new('ses', 'ses', [CompletionResultType]::ParameterValue, 'Manage manual ShadowStore transaction sessions')
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Manage, inspect, and execute saved AI plans')
            [CompletionResult]::new('pln', 'pln', [CompletionResultType]::ParameterValue, 'Manage, inspect, and execute saved AI plans')
            [CompletionResult]::new('p', 'p', [CompletionResultType]::ParameterValue, 'Manage, inspect, and execute saved AI plans')
            [CompletionResult]::new('index-worker', 'index-worker', [CompletionResultType]::ParameterValue, 'index-worker')
            [CompletionResult]::new('completions', 'completions', [CompletionResultType]::ParameterValue, 'Generate shell completion scripts')
            [CompletionResult]::new('man', 'man', [CompletionResultType]::ParameterValue, 'Generate man pages')
            [CompletionResult]::new('index', 'index', [CompletionResultType]::ParameterValue, 'Alias for curd doctor indexing')
            [CompletionResult]::new('idx', 'idx', [CompletionResultType]::ParameterValue, 'Alias for curd doctor indexing')
            [CompletionResult]::new('h', 'h', [CompletionResultType]::ParameterValue, 'Show help for CURD')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;mcp' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;init-agent' {
            [CompletionResult]::new('-n', '-n', [CompletionResultType]::ParameterName, 'Optional: Identifier for the agent (e.g., ''alpha'', ''claude_coder''). Use commas for multiple names')
            [CompletionResult]::new('--name', '--name', [CompletionResultType]::ParameterName, 'Optional: Identifier for the agent (e.g., ''alpha'', ''claude_coder''). Use commas for multiple names')
            [CompletionResult]::new('-r', '-r', [CompletionResultType]::ParameterName, 'Optional: Target harness to configure (gemini, cursor, claude_desktop, claude_code). Auto-detects if omitted')
            [CompletionResult]::new('--harness', '--harness', [CompletionResultType]::ParameterName, 'Optional: Target harness to configure (gemini, cursor, claude_desktop, claude_code). Auto-detects if omitted')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ina' {
            [CompletionResult]::new('-n', '-n', [CompletionResultType]::ParameterName, 'Optional: Identifier for the agent (e.g., ''alpha'', ''claude_coder''). Use commas for multiple names')
            [CompletionResult]::new('--name', '--name', [CompletionResultType]::ParameterName, 'Optional: Identifier for the agent (e.g., ''alpha'', ''claude_coder''). Use commas for multiple names')
            [CompletionResult]::new('-r', '-r', [CompletionResultType]::ParameterName, 'Optional: Target harness to configure (gemini, cursor, claude_desktop, claude_code). Auto-detects if omitted')
            [CompletionResult]::new('--harness', '--harness', [CompletionResultType]::ParameterName, 'Optional: Target harness to configure (gemini, cursor, claude_desktop, claude_code). Auto-detects if omitted')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;agt' {
            [CompletionResult]::new('-n', '-n', [CompletionResultType]::ParameterName, 'Optional: Identifier for the agent (e.g., ''alpha'', ''claude_coder''). Use commas for multiple names')
            [CompletionResult]::new('--name', '--name', [CompletionResultType]::ParameterName, 'Optional: Identifier for the agent (e.g., ''alpha'', ''claude_coder''). Use commas for multiple names')
            [CompletionResult]::new('-r', '-r', [CompletionResultType]::ParameterName, 'Optional: Target harness to configure (gemini, cursor, claude_desktop, claude_code). Auto-detects if omitted')
            [CompletionResult]::new('--harness', '--harness', [CompletionResultType]::ParameterName, 'Optional: Target harness to configure (gemini, cursor, claude_desktop, claude_code). Auto-detects if omitted')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;doctor' {
            [CompletionResult]::new('--max-total-ms', '--max-total-ms', [CompletionResultType]::ParameterName, 'Optional threshold: fail when index total_ms exceeds this value')
            [CompletionResult]::new('--max-parse-fail', '--max-parse-fail', [CompletionResultType]::ParameterName, 'Optional threshold: fail when parse_fail count exceeds this value')
            [CompletionResult]::new('--max-no-symbols-ratio', '--max-no-symbols-ratio', [CompletionResultType]::ParameterName, 'Optional threshold: fail when no_symbols/total_files exceeds this ratio (0.0..1.0)')
            [CompletionResult]::new('--max-skipped-large-ratio', '--max-skipped-large-ratio', [CompletionResultType]::ParameterName, 'Optional threshold: fail when skipped_large/total_files exceeds this ratio (0.0..1.0)')
            [CompletionResult]::new('--min-coverage-ratio', '--min-coverage-ratio', [CompletionResultType]::ParameterName, 'Optional threshold: fail when coverage_ratio is below this value (0.0..1.0)')
            [CompletionResult]::new('--require-coverage-state', '--require-coverage-state', [CompletionResultType]::ParameterName, 'Optional threshold: fail when coverage state does not match this exactly (e.g. ''full'')')
            [CompletionResult]::new('--min-symbol-count', '--min-symbol-count', [CompletionResultType]::ParameterName, 'Optional threshold: fail when total symbols found is below this count')
            [CompletionResult]::new('--min-symbols-per-k-files', '--min-symbols-per-k-files', [CompletionResultType]::ParameterName, 'Optional threshold: fail when symbol density is below this value')
            [CompletionResult]::new('--min-overlap-with-full', '--min-overlap-with-full', [CompletionResultType]::ParameterName, 'Optional threshold: fail when overlap with full index is below this value')
            [CompletionResult]::new('--report-out', '--report-out', [CompletionResultType]::ParameterName, 'Write report as JSON to this path')
            [CompletionResult]::new('--profile', '--profile', [CompletionResultType]::ParameterName, 'The profile of thresholds to apply (''ci_fast'' or ''ci_strict''). Overrides config')
            [CompletionResult]::new('--index-mode', '--index-mode', [CompletionResultType]::ParameterName, 'Optional index mode for this doctor run (e.g. ''lazy'', ''full'')')
            [CompletionResult]::new('--index-scope', '--index-scope', [CompletionResultType]::ParameterName, 'Optional index scope for this doctor run')
            [CompletionResult]::new('--index-max-file-size', '--index-max-file-size', [CompletionResultType]::ParameterName, 'Optional max file size in bytes for this doctor run')
            [CompletionResult]::new('--index-large-file-policy', '--index-large-file-policy', [CompletionResultType]::ParameterName, 'Optional large file policy for this doctor run')
            [CompletionResult]::new('--index-execution', '--index-execution', [CompletionResultType]::ParameterName, 'Optional index execution model for this doctor run')
            [CompletionResult]::new('--index-chunk-size', '--index-chunk-size', [CompletionResultType]::ParameterName, 'Optional chunk size for multiprocess mode')
            [CompletionResult]::new('--strict', '--strict', [CompletionResultType]::ParameterName, 'Fail with non-zero status when warnings/findings are present')
            [CompletionResult]::new('--parity-rerun', '--parity-rerun', [CompletionResultType]::ParameterName, 'Re-run the index build twice and ensure symbol counts and hashes match exactly')
            [CompletionResult]::new('--compare-with-full', '--compare-with-full', [CompletionResultType]::ParameterName, 'Compare index contents against a ''full'' mode run')
            [CompletionResult]::new('--profile-index', '--profile-index', [CompletionResultType]::ParameterName, 'Run indexing multiple times to generate a performance profile')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;dct' {
            [CompletionResult]::new('--max-total-ms', '--max-total-ms', [CompletionResultType]::ParameterName, 'Optional threshold: fail when index total_ms exceeds this value')
            [CompletionResult]::new('--max-parse-fail', '--max-parse-fail', [CompletionResultType]::ParameterName, 'Optional threshold: fail when parse_fail count exceeds this value')
            [CompletionResult]::new('--max-no-symbols-ratio', '--max-no-symbols-ratio', [CompletionResultType]::ParameterName, 'Optional threshold: fail when no_symbols/total_files exceeds this ratio (0.0..1.0)')
            [CompletionResult]::new('--max-skipped-large-ratio', '--max-skipped-large-ratio', [CompletionResultType]::ParameterName, 'Optional threshold: fail when skipped_large/total_files exceeds this ratio (0.0..1.0)')
            [CompletionResult]::new('--min-coverage-ratio', '--min-coverage-ratio', [CompletionResultType]::ParameterName, 'Optional threshold: fail when coverage_ratio is below this value (0.0..1.0)')
            [CompletionResult]::new('--require-coverage-state', '--require-coverage-state', [CompletionResultType]::ParameterName, 'Optional threshold: fail when coverage state does not match this exactly (e.g. ''full'')')
            [CompletionResult]::new('--min-symbol-count', '--min-symbol-count', [CompletionResultType]::ParameterName, 'Optional threshold: fail when total symbols found is below this count')
            [CompletionResult]::new('--min-symbols-per-k-files', '--min-symbols-per-k-files', [CompletionResultType]::ParameterName, 'Optional threshold: fail when symbol density is below this value')
            [CompletionResult]::new('--min-overlap-with-full', '--min-overlap-with-full', [CompletionResultType]::ParameterName, 'Optional threshold: fail when overlap with full index is below this value')
            [CompletionResult]::new('--report-out', '--report-out', [CompletionResultType]::ParameterName, 'Write report as JSON to this path')
            [CompletionResult]::new('--profile', '--profile', [CompletionResultType]::ParameterName, 'The profile of thresholds to apply (''ci_fast'' or ''ci_strict''). Overrides config')
            [CompletionResult]::new('--index-mode', '--index-mode', [CompletionResultType]::ParameterName, 'Optional index mode for this doctor run (e.g. ''lazy'', ''full'')')
            [CompletionResult]::new('--index-scope', '--index-scope', [CompletionResultType]::ParameterName, 'Optional index scope for this doctor run')
            [CompletionResult]::new('--index-max-file-size', '--index-max-file-size', [CompletionResultType]::ParameterName, 'Optional max file size in bytes for this doctor run')
            [CompletionResult]::new('--index-large-file-policy', '--index-large-file-policy', [CompletionResultType]::ParameterName, 'Optional large file policy for this doctor run')
            [CompletionResult]::new('--index-execution', '--index-execution', [CompletionResultType]::ParameterName, 'Optional index execution model for this doctor run')
            [CompletionResult]::new('--index-chunk-size', '--index-chunk-size', [CompletionResultType]::ParameterName, 'Optional chunk size for multiprocess mode')
            [CompletionResult]::new('--strict', '--strict', [CompletionResultType]::ParameterName, 'Fail with non-zero status when warnings/findings are present')
            [CompletionResult]::new('--parity-rerun', '--parity-rerun', [CompletionResultType]::ParameterName, 'Re-run the index build twice and ensure symbol counts and hashes match exactly')
            [CompletionResult]::new('--compare-with-full', '--compare-with-full', [CompletionResultType]::ParameterName, 'Compare index contents against a ''full'' mode run')
            [CompletionResult]::new('--profile-index', '--profile-index', [CompletionResultType]::ParameterName, 'Run indexing multiple times to generate a performance profile')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;build' {
            [CompletionResult]::new('--adapter', '--adapter', [CompletionResultType]::ParameterName, 'Build adapter override (e.g. cargo, cmake, ninja, make, uv, poetry, pip, conda, mamba, npm, yarn, pnpm, bun)')
            [CompletionResult]::new('--profile', '--profile', [CompletionResultType]::ParameterName, 'Build profile (debug|release)')
            [CompletionResult]::new('--execute', '--execute', [CompletionResultType]::ParameterName, 'Execute planned commands (default: true)')
            [CompletionResult]::new('-c', '-c', [CompletionResultType]::ParameterName, 'Custom command to run directly, overriding adapters (e.g. `pixi run dev2`)')
            [CompletionResult]::new('--command', '--command', [CompletionResultType]::ParameterName, 'Custom command to run directly, overriding adapters (e.g. `pixi run dev2`)')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Only show the build plan, do not execute')
            [CompletionResult]::new('--allow-untrusted', '--allow-untrusted', [CompletionResultType]::ParameterName, 'Allow execution of custom adapters defined in workspace settings.toml without prompt')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output results as JSON')
            [CompletionResult]::new('--zig', '--zig', [CompletionResultType]::ParameterName, 'Use cargo-zigbuild instead of cargo build')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;bld' {
            [CompletionResult]::new('--adapter', '--adapter', [CompletionResultType]::ParameterName, 'Build adapter override (e.g. cargo, cmake, ninja, make, uv, poetry, pip, conda, mamba, npm, yarn, pnpm, bun)')
            [CompletionResult]::new('--profile', '--profile', [CompletionResultType]::ParameterName, 'Build profile (debug|release)')
            [CompletionResult]::new('--execute', '--execute', [CompletionResultType]::ParameterName, 'Execute planned commands (default: true)')
            [CompletionResult]::new('-c', '-c', [CompletionResultType]::ParameterName, 'Custom command to run directly, overriding adapters (e.g. `pixi run dev2`)')
            [CompletionResult]::new('--command', '--command', [CompletionResultType]::ParameterName, 'Custom command to run directly, overriding adapters (e.g. `pixi run dev2`)')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Only show the build plan, do not execute')
            [CompletionResult]::new('--allow-untrusted', '--allow-untrusted', [CompletionResultType]::ParameterName, 'Allow execution of custom adapters defined in workspace settings.toml without prompt')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output results as JSON')
            [CompletionResult]::new('--zig', '--zig', [CompletionResultType]::ParameterName, 'Use cargo-zigbuild instead of cargo build')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;b' {
            [CompletionResult]::new('--adapter', '--adapter', [CompletionResultType]::ParameterName, 'Build adapter override (e.g. cargo, cmake, ninja, make, uv, poetry, pip, conda, mamba, npm, yarn, pnpm, bun)')
            [CompletionResult]::new('--profile', '--profile', [CompletionResultType]::ParameterName, 'Build profile (debug|release)')
            [CompletionResult]::new('--execute', '--execute', [CompletionResultType]::ParameterName, 'Execute planned commands (default: true)')
            [CompletionResult]::new('-c', '-c', [CompletionResultType]::ParameterName, 'Custom command to run directly, overriding adapters (e.g. `pixi run dev2`)')
            [CompletionResult]::new('--command', '--command', [CompletionResultType]::ParameterName, 'Custom command to run directly, overriding adapters (e.g. `pixi run dev2`)')
            [CompletionResult]::new('--plan', '--plan', [CompletionResultType]::ParameterName, 'Only show the build plan, do not execute')
            [CompletionResult]::new('--allow-untrusted', '--allow-untrusted', [CompletionResultType]::ParameterName, 'Allow execution of custom adapters defined in workspace settings.toml without prompt')
            [CompletionResult]::new('--json', '--json', [CompletionResultType]::ParameterName, 'Output results as JSON')
            [CompletionResult]::new('--zig', '--zig', [CompletionResultType]::ParameterName, 'Use cargo-zigbuild instead of cargo build')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;hook' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;hok' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;diff' {
            [CompletionResult]::new('--symbol', '--symbol', [CompletionResultType]::ParameterName, 'Optional specific symbol to diff')
            [CompletionResult]::new('--semantic', '--semantic', [CompletionResultType]::ParameterName, 'Semantic AST-level diff')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;dif' {
            [CompletionResult]::new('--symbol', '--symbol', [CompletionResultType]::ParameterName, 'Optional specific symbol to diff')
            [CompletionResult]::new('--semantic', '--semantic', [CompletionResultType]::ParameterName, 'Semantic AST-level diff')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;refactor' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('rename', 'rename', [CompletionResultType]::ParameterValue, 'Rename a function/class/variable')
            [CompletionResult]::new('move', 'move', [CompletionResultType]::ParameterValue, 'Move a function between files')
            [CompletionResult]::new('extract', 'extract', [CompletionResultType]::ParameterValue, 'Extract code into a new function')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;ref' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('rename', 'rename', [CompletionResultType]::ParameterValue, 'Rename a function/class/variable')
            [CompletionResult]::new('move', 'move', [CompletionResultType]::ParameterValue, 'Move a function between files')
            [CompletionResult]::new('extract', 'extract', [CompletionResultType]::ParameterValue, 'Extract code into a new function')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;refactor;rename' {
            [CompletionResult]::new('--lsp', '--lsp', [CompletionResultType]::ParameterName, 'Optional: Use an external LSP server for type-aware renaming (e.g., ''rust-analyzer'')')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ref;rename' {
            [CompletionResult]::new('--lsp', '--lsp', [CompletionResultType]::ParameterName, 'Optional: Use an external LSP server for type-aware renaming (e.g., ''rust-analyzer'')')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;refactor;move' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ref;move' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;refactor;extract' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ref;extract' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;refactor;help' {
            [CompletionResult]::new('rename', 'rename', [CompletionResultType]::ParameterValue, 'Rename a function/class/variable')
            [CompletionResult]::new('move', 'move', [CompletionResultType]::ParameterValue, 'Move a function between files')
            [CompletionResult]::new('extract', 'extract', [CompletionResultType]::ParameterValue, 'Extract code into a new function')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;refactor;help;rename' {
            break
        }
        'curd;refactor;help;move' {
            break
        }
        'curd;refactor;help;extract' {
            break
        }
        'curd;refactor;help;help' {
            break
        }
        'curd;ref;help' {
            [CompletionResult]::new('rename', 'rename', [CompletionResultType]::ParameterValue, 'Rename a function/class/variable')
            [CompletionResult]::new('move', 'move', [CompletionResultType]::ParameterValue, 'Move a function between files')
            [CompletionResult]::new('extract', 'extract', [CompletionResultType]::ParameterValue, 'Extract code into a new function')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;ref;help;rename' {
            break
        }
        'curd;ref;help;move' {
            break
        }
        'curd;ref;help;extract' {
            break
        }
        'curd;ref;help;help' {
            break
        }
        'curd;context' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the primary workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add an external repository context')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a linked context by alias or path')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List all currently linked contexts')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;ctx' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the primary workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add an external repository context')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a linked context by alias or path')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List all currently linked contexts')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;context;add' {
            [CompletionResult]::new('--alias', '--alias', [CompletionResultType]::ParameterName, 'Optional alias for this context (auto-generated if omitted)')
            [CompletionResult]::new('--index', '--index', [CompletionResultType]::ParameterName, 'Link context in Index mode (Search and Graph only, no file reads)')
            [CompletionResult]::new('--read', '--read', [CompletionResultType]::ParameterName, 'Link context in Read mode (Read-only, no writes)')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ctx;add' {
            [CompletionResult]::new('--alias', '--alias', [CompletionResultType]::ParameterName, 'Optional alias for this context (auto-generated if omitted)')
            [CompletionResult]::new('--index', '--index', [CompletionResultType]::ParameterName, 'Link context in Index mode (Search and Graph only, no file reads)')
            [CompletionResult]::new('--read', '--read', [CompletionResultType]::ParameterName, 'Link context in Read mode (Read-only, no writes)')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;context;remove' {
            [CompletionResult]::new('--force', '--force', [CompletionResultType]::ParameterName, 'Force removal even if there are dangling dependencies in the primary workspace')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ctx;remove' {
            [CompletionResult]::new('--force', '--force', [CompletionResultType]::ParameterName, 'Force removal even if there are dangling dependencies in the primary workspace')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;context;list' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ctx;list' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;context;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add an external repository context')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a linked context by alias or path')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List all currently linked contexts')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;context;help;add' {
            break
        }
        'curd;context;help;remove' {
            break
        }
        'curd;context;help;list' {
            break
        }
        'curd;context;help;help' {
            break
        }
        'curd;ctx;help' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add an external repository context')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a linked context by alias or path')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List all currently linked contexts')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;ctx;help;add' {
            break
        }
        'curd;ctx;help;remove' {
            break
        }
        'curd;ctx;help;list' {
            break
        }
        'curd;ctx;help;help' {
            break
        }
        'curd;init' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ini' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;config' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show current workspace configuration')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Set a configuration value (e.g. `set index.mode fast`)')
            [CompletionResult]::new('unset', 'unset', [CompletionResultType]::ParameterValue, 'Remove a configuration value')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;cfg' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show current workspace configuration')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Set a configuration value (e.g. `set index.mode fast`)')
            [CompletionResult]::new('unset', 'unset', [CompletionResultType]::ParameterValue, 'Remove a configuration value')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;config;show' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;cfg;show' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;config;set' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;cfg;set' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;config;unset' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;cfg;unset' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;config;help' {
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show current workspace configuration')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Set a configuration value (e.g. `set index.mode fast`)')
            [CompletionResult]::new('unset', 'unset', [CompletionResultType]::ParameterValue, 'Remove a configuration value')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;config;help;show' {
            break
        }
        'curd;config;help;set' {
            break
        }
        'curd;config;help;unset' {
            break
        }
        'curd;config;help;help' {
            break
        }
        'curd;cfg;help' {
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show current workspace configuration')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Set a configuration value (e.g. `set index.mode fast`)')
            [CompletionResult]::new('unset', 'unset', [CompletionResultType]::ParameterValue, 'Remove a configuration value')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;cfg;help;show' {
            break
        }
        'curd;cfg;help;set' {
            break
        }
        'curd;cfg;help;unset' {
            break
        }
        'curd;cfg;help;help' {
            break
        }
        'curd;plugin-language' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List installed plugin packages')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Install a signed plugin archive')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove an installed plugin package')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;plang' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List installed plugin packages')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Install a signed plugin archive')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove an installed plugin package')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;plugin-language;list' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plang;list' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plugin-language;add' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plang;add' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plugin-language;remove' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plang;remove' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plugin-language;help' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List installed plugin packages')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Install a signed plugin archive')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove an installed plugin package')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;plugin-language;help;list' {
            break
        }
        'curd;plugin-language;help;add' {
            break
        }
        'curd;plugin-language;help;remove' {
            break
        }
        'curd;plugin-language;help;help' {
            break
        }
        'curd;plang;help' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List installed plugin packages')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Install a signed plugin archive')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove an installed plugin package')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;plang;help;list' {
            break
        }
        'curd;plang;help;add' {
            break
        }
        'curd;plang;help;remove' {
            break
        }
        'curd;plang;help;help' {
            break
        }
        'curd;plugin-tool' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List installed plugin packages')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Install a signed plugin archive')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove an installed plugin package')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;ptool' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List installed plugin packages')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Install a signed plugin archive')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove an installed plugin package')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;plugin-tool;list' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ptool;list' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plugin-tool;add' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ptool;add' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plugin-tool;remove' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ptool;remove' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plugin-tool;help' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List installed plugin packages')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Install a signed plugin archive')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove an installed plugin package')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;plugin-tool;help;list' {
            break
        }
        'curd;plugin-tool;help;add' {
            break
        }
        'curd;plugin-tool;help;remove' {
            break
        }
        'curd;plugin-tool;help;help' {
            break
        }
        'curd;ptool;help' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List installed plugin packages')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Install a signed plugin archive')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove an installed plugin package')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;ptool;help;list' {
            break
        }
        'curd;ptool;help;add' {
            break
        }
        'curd;ptool;help;remove' {
            break
        }
        'curd;ptool;help;help' {
            break
        }
        'curd;plugin-trust' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List trusted signing keys')
            [CompletionResult]::new('get', 'get', [CompletionResultType]::ParameterValue, 'Get one trusted signing key')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add or replace a trusted signing key')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a trusted signing key')
            [CompletionResult]::new('enable', 'enable', [CompletionResultType]::ParameterValue, 'Enable a trusted signing key')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Disable a trusted signing key')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;ptrust' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List trusted signing keys')
            [CompletionResult]::new('get', 'get', [CompletionResultType]::ParameterValue, 'Get one trusted signing key')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add or replace a trusted signing key')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a trusted signing key')
            [CompletionResult]::new('enable', 'enable', [CompletionResultType]::ParameterValue, 'Enable a trusted signing key')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Disable a trusted signing key')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;plugin-trust;list' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ptrust;list' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plugin-trust;get' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ptrust;get' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plugin-trust;add' {
            [CompletionResult]::new('--label', '--label', [CompletionResultType]::ParameterName, 'Optional label')
            [CompletionResult]::new('--kind', '--kind', [CompletionResultType]::ParameterName, 'Allowed plugin kinds for this key')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ptrust;add' {
            [CompletionResult]::new('--label', '--label', [CompletionResultType]::ParameterName, 'Optional label')
            [CompletionResult]::new('--kind', '--kind', [CompletionResultType]::ParameterName, 'Allowed plugin kinds for this key')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plugin-trust;remove' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ptrust;remove' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plugin-trust;enable' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ptrust;enable' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plugin-trust;disable' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ptrust;disable' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plugin-trust;help' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List trusted signing keys')
            [CompletionResult]::new('get', 'get', [CompletionResultType]::ParameterValue, 'Get one trusted signing key')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add or replace a trusted signing key')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a trusted signing key')
            [CompletionResult]::new('enable', 'enable', [CompletionResultType]::ParameterValue, 'Enable a trusted signing key')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Disable a trusted signing key')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;plugin-trust;help;list' {
            break
        }
        'curd;plugin-trust;help;get' {
            break
        }
        'curd;plugin-trust;help;add' {
            break
        }
        'curd;plugin-trust;help;remove' {
            break
        }
        'curd;plugin-trust;help;enable' {
            break
        }
        'curd;plugin-trust;help;disable' {
            break
        }
        'curd;plugin-trust;help;help' {
            break
        }
        'curd;ptrust;help' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List trusted signing keys')
            [CompletionResult]::new('get', 'get', [CompletionResultType]::ParameterValue, 'Get one trusted signing key')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add or replace a trusted signing key')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a trusted signing key')
            [CompletionResult]::new('enable', 'enable', [CompletionResultType]::ParameterValue, 'Enable a trusted signing key')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Disable a trusted signing key')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;ptrust;help;list' {
            break
        }
        'curd;ptrust;help;get' {
            break
        }
        'curd;ptrust;help;add' {
            break
        }
        'curd;ptrust;help;remove' {
            break
        }
        'curd;ptrust;help;enable' {
            break
        }
        'curd;ptrust;help;disable' {
            break
        }
        'curd;ptrust;help;help' {
            break
        }
        'curd;detach' {
            [CompletionResult]::new('--shadow', '--shadow', [CompletionResultType]::ParameterName, 'How to handle an active shadow transaction before detaching')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;det' {
            [CompletionResult]::new('--shadow', '--shadow', [CompletionResultType]::ParameterName, 'How to handle an active shadow transaction before detaching')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;delete' {
            [CompletionResult]::new('--shadow', '--shadow', [CompletionResultType]::ParameterName, 'How to handle an active shadow transaction before deleting CURD state')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Force skip confirmation')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Force skip confirmation')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;del' {
            [CompletionResult]::new('--shadow', '--shadow', [CompletionResultType]::ParameterName, 'How to handle an active shadow transaction before deleting CURD state')
            [CompletionResult]::new('-y', '-y', [CompletionResultType]::ParameterName, 'Force skip confirmation')
            [CompletionResult]::new('--yes', '--yes', [CompletionResultType]::ParameterName, 'Force skip confirmation')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;status' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;st' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;sts' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;log' {
            [CompletionResult]::new('-l', '-l', [CompletionResultType]::ParameterName, 'Number of recent entries to show')
            [CompletionResult]::new('--limit', '--limit', [CompletionResultType]::ParameterName, 'Number of recent entries to show')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;repl' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;rpl' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;run' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('--arg', '--arg', [CompletionResultType]::ParameterName, 'Script argument override in key=value form')
            [CompletionResult]::new('--profile', '--profile', [CompletionResultType]::ParameterName, 'Optional profile override')
            [CompletionResult]::new('--out', '--out', [CompletionResultType]::ParameterName, 'Output path for `curd run compile ...`')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;test' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Show detailed list of broken links and dead zones')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Show detailed list of broken links and dead zones')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;tst' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-v', '-v', [CompletionResultType]::ParameterName, 'Show detailed list of broken links and dead zones')
            [CompletionResult]::new('--verbose', '--verbose', [CompletionResultType]::ParameterName, 'Show detailed list of broken links and dead zones')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;search' {
            [CompletionResult]::new('--kind', '--kind', [CompletionResultType]::ParameterName, 'Optional kind filter (e.g. ''function'', ''class'')')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;sch' {
            [CompletionResult]::new('--kind', '--kind', [CompletionResultType]::ParameterName, 'Optional kind filter (e.g. ''function'', ''class'')')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;s' {
            [CompletionResult]::new('--kind', '--kind', [CompletionResultType]::ParameterName, 'Optional kind filter (e.g. ''function'', ''class'')')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;graph' {
            [CompletionResult]::new('--depth', '--depth', [CompletionResultType]::ParameterName, 'The traversal depth')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;g' {
            [CompletionResult]::new('--depth', '--depth', [CompletionResultType]::ParameterName, 'The traversal depth')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;read' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;red' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;edit' {
            [CompletionResult]::new('-a', '-a', [CompletionResultType]::ParameterName, 'The action to perform (''upsert'' or ''delete'')')
            [CompletionResult]::new('--action', '--action', [CompletionResultType]::ParameterName, 'The action to perform (''upsert'' or ''delete'')')
            [CompletionResult]::new('-c', '-c', [CompletionResultType]::ParameterName, 'The literal code to insert (ignored if action is ''delete'')')
            [CompletionResult]::new('--code', '--code', [CompletionResultType]::ParameterName, 'The literal code to insert (ignored if action is ''delete'')')
            [CompletionResult]::new('--base-state-hash', '--base-state-hash', [CompletionResultType]::ParameterName, 'Optional: The expected Merkle Root of the workspace state for optimistic concurrency control')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;edt' {
            [CompletionResult]::new('-a', '-a', [CompletionResultType]::ParameterName, 'The action to perform (''upsert'' or ''delete'')')
            [CompletionResult]::new('--action', '--action', [CompletionResultType]::ParameterName, 'The action to perform (''upsert'' or ''delete'')')
            [CompletionResult]::new('-c', '-c', [CompletionResultType]::ParameterName, 'The literal code to insert (ignored if action is ''delete'')')
            [CompletionResult]::new('--code', '--code', [CompletionResultType]::ParameterName, 'The literal code to insert (ignored if action is ''delete'')')
            [CompletionResult]::new('--base-state-hash', '--base-state-hash', [CompletionResultType]::ParameterName, 'Optional: The expected Merkle Root of the workspace state for optimistic concurrency control')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;e' {
            [CompletionResult]::new('-a', '-a', [CompletionResultType]::ParameterName, 'The action to perform (''upsert'' or ''delete'')')
            [CompletionResult]::new('--action', '--action', [CompletionResultType]::ParameterName, 'The action to perform (''upsert'' or ''delete'')')
            [CompletionResult]::new('-c', '-c', [CompletionResultType]::ParameterName, 'The literal code to insert (ignored if action is ''delete'')')
            [CompletionResult]::new('--code', '--code', [CompletionResultType]::ParameterName, 'The literal code to insert (ignored if action is ''delete'')')
            [CompletionResult]::new('--base-state-hash', '--base-state-hash', [CompletionResultType]::ParameterName, 'Optional: The expected Merkle Root of the workspace state for optimistic concurrency control')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;diagram' {
            [CompletionResult]::new('-u', '-u', [CompletionResultType]::ParameterName, 'Symbol URIs to include as roots')
            [CompletionResult]::new('--uris', '--uris', [CompletionResultType]::ParameterName, 'Symbol URIs to include as roots')
            [CompletionResult]::new('-r', '-r', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-f', '-f', [CompletionResultType]::ParameterName, 'Output format (ascii, svg, dot, mermaid)')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format (ascii, svg, dot, mermaid)')
            [CompletionResult]::new('--depth', '--depth', [CompletionResultType]::ParameterName, 'Traversal depth for dependencies')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;dia' {
            [CompletionResult]::new('-u', '-u', [CompletionResultType]::ParameterName, 'Symbol URIs to include as roots')
            [CompletionResult]::new('--uris', '--uris', [CompletionResultType]::ParameterName, 'Symbol URIs to include as roots')
            [CompletionResult]::new('-r', '-r', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-f', '-f', [CompletionResultType]::ParameterName, 'Output format (ascii, svg, dot, mermaid)')
            [CompletionResult]::new('--format', '--format', [CompletionResultType]::ParameterName, 'Output format (ascii, svg, dot, mermaid)')
            [CompletionResult]::new('--depth', '--depth', [CompletionResultType]::ParameterName, 'Traversal depth for dependencies')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;session' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('begin', 'begin', [CompletionResultType]::ParameterValue, 'Open a new shadow transaction')
            [CompletionResult]::new('review', 'review', [CompletionResultType]::ParameterValue, 'View changes and architectural impact of the current session')
            [CompletionResult]::new('log', 'log', [CompletionResultType]::ParameterValue, 'View a detailed log of all tool calls and their results')
            [CompletionResult]::new('commit', 'commit', [CompletionResultType]::ParameterValue, 'Commit the active shadow transaction to disk')
            [CompletionResult]::new('rollback', 'rollback', [CompletionResultType]::ParameterValue, 'Discard the active shadow transaction')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;ses' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('begin', 'begin', [CompletionResultType]::ParameterValue, 'Open a new shadow transaction')
            [CompletionResult]::new('review', 'review', [CompletionResultType]::ParameterValue, 'View changes and architectural impact of the current session')
            [CompletionResult]::new('log', 'log', [CompletionResultType]::ParameterValue, 'View a detailed log of all tool calls and their results')
            [CompletionResult]::new('commit', 'commit', [CompletionResultType]::ParameterValue, 'Commit the active shadow transaction to disk')
            [CompletionResult]::new('rollback', 'rollback', [CompletionResultType]::ParameterValue, 'Discard the active shadow transaction')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;session;begin' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ses;begin' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;session;review' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ses;review' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;session;log' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ses;log' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;session;commit' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ses;commit' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;session;rollback' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;ses;rollback' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;session;help' {
            [CompletionResult]::new('begin', 'begin', [CompletionResultType]::ParameterValue, 'Open a new shadow transaction')
            [CompletionResult]::new('review', 'review', [CompletionResultType]::ParameterValue, 'View changes and architectural impact of the current session')
            [CompletionResult]::new('log', 'log', [CompletionResultType]::ParameterValue, 'View a detailed log of all tool calls and their results')
            [CompletionResult]::new('commit', 'commit', [CompletionResultType]::ParameterValue, 'Commit the active shadow transaction to disk')
            [CompletionResult]::new('rollback', 'rollback', [CompletionResultType]::ParameterValue, 'Discard the active shadow transaction')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;session;help;begin' {
            break
        }
        'curd;session;help;review' {
            break
        }
        'curd;session;help;log' {
            break
        }
        'curd;session;help;commit' {
            break
        }
        'curd;session;help;rollback' {
            break
        }
        'curd;session;help;help' {
            break
        }
        'curd;ses;help' {
            [CompletionResult]::new('begin', 'begin', [CompletionResultType]::ParameterValue, 'Open a new shadow transaction')
            [CompletionResult]::new('review', 'review', [CompletionResultType]::ParameterValue, 'View changes and architectural impact of the current session')
            [CompletionResult]::new('log', 'log', [CompletionResultType]::ParameterValue, 'View a detailed log of all tool calls and their results')
            [CompletionResult]::new('commit', 'commit', [CompletionResultType]::ParameterValue, 'Commit the active shadow transaction to disk')
            [CompletionResult]::new('rollback', 'rollback', [CompletionResultType]::ParameterValue, 'Discard the active shadow transaction')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;ses;help;begin' {
            break
        }
        'curd;ses;help;review' {
            break
        }
        'curd;ses;help;log' {
            break
        }
        'curd;ses;help;commit' {
            break
        }
        'curd;ses;help;rollback' {
            break
        }
        'curd;ses;help;help' {
            break
        }
        'curd;plan' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List all saved plans in the workspace')
            [CompletionResult]::new('read', 'read', [CompletionResultType]::ParameterValue, 'Read a specific plan and format it as a readable execution tree')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Edit a saved plan or compiled script artifact interactively')
            [CompletionResult]::new('impl', 'impl', [CompletionResultType]::ParameterValue, 'Execute a saved plan')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;pln' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List all saved plans in the workspace')
            [CompletionResult]::new('read', 'read', [CompletionResultType]::ParameterValue, 'Read a specific plan and format it as a readable execution tree')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Edit a saved plan or compiled script artifact interactively')
            [CompletionResult]::new('impl', 'impl', [CompletionResultType]::ParameterValue, 'Execute a saved plan')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;p' {
            [CompletionResult]::new('--root', '--root', [CompletionResultType]::ParameterName, 'Path to the workspace root')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List all saved plans in the workspace')
            [CompletionResult]::new('read', 'read', [CompletionResultType]::ParameterValue, 'Read a specific plan and format it as a readable execution tree')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Edit a saved plan or compiled script artifact interactively')
            [CompletionResult]::new('impl', 'impl', [CompletionResultType]::ParameterValue, 'Execute a saved plan')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;plan;list' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;pln;list' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;p;list' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plan;read' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;pln;read' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;p;read' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plan;edit' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;pln;edit' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;p;edit' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plan;impl' {
            [CompletionResult]::new('--session', '--session', [CompletionResultType]::ParameterName, 'The UUID of the active session this plan belongs to')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Perform a dry-run validation (simulate) instead of executing')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;pln;impl' {
            [CompletionResult]::new('--session', '--session', [CompletionResultType]::ParameterName, 'The UUID of the active session this plan belongs to')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Perform a dry-run validation (simulate) instead of executing')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;p;impl' {
            [CompletionResult]::new('--session', '--session', [CompletionResultType]::ParameterName, 'The UUID of the active session this plan belongs to')
            [CompletionResult]::new('--dry-run', '--dry-run', [CompletionResultType]::ParameterName, 'Perform a dry-run validation (simulate) instead of executing')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;plan;help' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List all saved plans in the workspace')
            [CompletionResult]::new('read', 'read', [CompletionResultType]::ParameterValue, 'Read a specific plan and format it as a readable execution tree')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Edit a saved plan or compiled script artifact interactively')
            [CompletionResult]::new('impl', 'impl', [CompletionResultType]::ParameterValue, 'Execute a saved plan')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;plan;help;list' {
            break
        }
        'curd;plan;help;read' {
            break
        }
        'curd;plan;help;edit' {
            break
        }
        'curd;plan;help;impl' {
            break
        }
        'curd;plan;help;help' {
            break
        }
        'curd;pln;help' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List all saved plans in the workspace')
            [CompletionResult]::new('read', 'read', [CompletionResultType]::ParameterValue, 'Read a specific plan and format it as a readable execution tree')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Edit a saved plan or compiled script artifact interactively')
            [CompletionResult]::new('impl', 'impl', [CompletionResultType]::ParameterValue, 'Execute a saved plan')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;pln;help;list' {
            break
        }
        'curd;pln;help;read' {
            break
        }
        'curd;pln;help;edit' {
            break
        }
        'curd;pln;help;impl' {
            break
        }
        'curd;pln;help;help' {
            break
        }
        'curd;p;help' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List all saved plans in the workspace')
            [CompletionResult]::new('read', 'read', [CompletionResultType]::ParameterValue, 'Read a specific plan and format it as a readable execution tree')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Edit a saved plan or compiled script artifact interactively')
            [CompletionResult]::new('impl', 'impl', [CompletionResultType]::ParameterValue, 'Execute a saved plan')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;p;help;list' {
            break
        }
        'curd;p;help;read' {
            break
        }
        'curd;p;help;edit' {
            break
        }
        'curd;p;help;impl' {
            break
        }
        'curd;p;help;help' {
            break
        }
        'curd;index-worker' {
            [CompletionResult]::new('--request', '--request', [CompletionResultType]::ParameterName, 'request')
            [CompletionResult]::new('--response', '--response', [CompletionResultType]::ParameterName, 'response')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;completions' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;man' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;index' {
            [CompletionResult]::new('--max-total-ms', '--max-total-ms', [CompletionResultType]::ParameterName, 'Optional threshold: fail when index total_ms exceeds this value')
            [CompletionResult]::new('--max-parse-fail', '--max-parse-fail', [CompletionResultType]::ParameterName, 'Optional threshold: fail when parse_fail count exceeds this value')
            [CompletionResult]::new('--max-no-symbols-ratio', '--max-no-symbols-ratio', [CompletionResultType]::ParameterName, 'Optional threshold: fail when no_symbols/total_files exceeds this ratio (0.0..1.0)')
            [CompletionResult]::new('--max-skipped-large-ratio', '--max-skipped-large-ratio', [CompletionResultType]::ParameterName, 'Optional threshold: fail when skipped_large/total_files exceeds this ratio (0.0..1.0)')
            [CompletionResult]::new('--min-coverage-ratio', '--min-coverage-ratio', [CompletionResultType]::ParameterName, 'Optional threshold: fail when coverage_ratio is below this value (0.0..1.0)')
            [CompletionResult]::new('--require-coverage-state', '--require-coverage-state', [CompletionResultType]::ParameterName, 'Optional threshold: fail when coverage state does not match this exactly (e.g. ''full'')')
            [CompletionResult]::new('--min-symbol-count', '--min-symbol-count', [CompletionResultType]::ParameterName, 'Optional threshold: fail when total symbols found is below this count')
            [CompletionResult]::new('--min-symbols-per-k-files', '--min-symbols-per-k-files', [CompletionResultType]::ParameterName, 'Optional threshold: fail when symbol density is below this value')
            [CompletionResult]::new('--min-overlap-with-full', '--min-overlap-with-full', [CompletionResultType]::ParameterName, 'Optional threshold: fail when overlap with full index is below this value')
            [CompletionResult]::new('--report-out', '--report-out', [CompletionResultType]::ParameterName, 'Write report as JSON to this path')
            [CompletionResult]::new('--profile', '--profile', [CompletionResultType]::ParameterName, 'The profile of thresholds to apply (''ci_fast'' or ''ci_strict''). Overrides config')
            [CompletionResult]::new('--index-mode', '--index-mode', [CompletionResultType]::ParameterName, 'Optional index mode for this doctor run (e.g. ''lazy'', ''full'')')
            [CompletionResult]::new('--index-scope', '--index-scope', [CompletionResultType]::ParameterName, 'Optional index scope for this doctor run')
            [CompletionResult]::new('--index-max-file-size', '--index-max-file-size', [CompletionResultType]::ParameterName, 'Optional max file size in bytes for this doctor run')
            [CompletionResult]::new('--index-large-file-policy', '--index-large-file-policy', [CompletionResultType]::ParameterName, 'Optional large file policy for this doctor run')
            [CompletionResult]::new('--index-execution', '--index-execution', [CompletionResultType]::ParameterName, 'Optional index execution model for this doctor run')
            [CompletionResult]::new('--index-chunk-size', '--index-chunk-size', [CompletionResultType]::ParameterName, 'Optional chunk size for multiprocess mode')
            [CompletionResult]::new('--strict', '--strict', [CompletionResultType]::ParameterName, 'Fail with non-zero status when warnings/findings are present')
            [CompletionResult]::new('--parity-rerun', '--parity-rerun', [CompletionResultType]::ParameterName, 'Re-run the index build twice and ensure symbol counts and hashes match exactly')
            [CompletionResult]::new('--compare-with-full', '--compare-with-full', [CompletionResultType]::ParameterName, 'Compare index contents against a ''full'' mode run')
            [CompletionResult]::new('--profile-index', '--profile-index', [CompletionResultType]::ParameterName, 'Run indexing multiple times to generate a performance profile')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;idx' {
            [CompletionResult]::new('--max-total-ms', '--max-total-ms', [CompletionResultType]::ParameterName, 'Optional threshold: fail when index total_ms exceeds this value')
            [CompletionResult]::new('--max-parse-fail', '--max-parse-fail', [CompletionResultType]::ParameterName, 'Optional threshold: fail when parse_fail count exceeds this value')
            [CompletionResult]::new('--max-no-symbols-ratio', '--max-no-symbols-ratio', [CompletionResultType]::ParameterName, 'Optional threshold: fail when no_symbols/total_files exceeds this ratio (0.0..1.0)')
            [CompletionResult]::new('--max-skipped-large-ratio', '--max-skipped-large-ratio', [CompletionResultType]::ParameterName, 'Optional threshold: fail when skipped_large/total_files exceeds this ratio (0.0..1.0)')
            [CompletionResult]::new('--min-coverage-ratio', '--min-coverage-ratio', [CompletionResultType]::ParameterName, 'Optional threshold: fail when coverage_ratio is below this value (0.0..1.0)')
            [CompletionResult]::new('--require-coverage-state', '--require-coverage-state', [CompletionResultType]::ParameterName, 'Optional threshold: fail when coverage state does not match this exactly (e.g. ''full'')')
            [CompletionResult]::new('--min-symbol-count', '--min-symbol-count', [CompletionResultType]::ParameterName, 'Optional threshold: fail when total symbols found is below this count')
            [CompletionResult]::new('--min-symbols-per-k-files', '--min-symbols-per-k-files', [CompletionResultType]::ParameterName, 'Optional threshold: fail when symbol density is below this value')
            [CompletionResult]::new('--min-overlap-with-full', '--min-overlap-with-full', [CompletionResultType]::ParameterName, 'Optional threshold: fail when overlap with full index is below this value')
            [CompletionResult]::new('--report-out', '--report-out', [CompletionResultType]::ParameterName, 'Write report as JSON to this path')
            [CompletionResult]::new('--profile', '--profile', [CompletionResultType]::ParameterName, 'The profile of thresholds to apply (''ci_fast'' or ''ci_strict''). Overrides config')
            [CompletionResult]::new('--index-mode', '--index-mode', [CompletionResultType]::ParameterName, 'Optional index mode for this doctor run (e.g. ''lazy'', ''full'')')
            [CompletionResult]::new('--index-scope', '--index-scope', [CompletionResultType]::ParameterName, 'Optional index scope for this doctor run')
            [CompletionResult]::new('--index-max-file-size', '--index-max-file-size', [CompletionResultType]::ParameterName, 'Optional max file size in bytes for this doctor run')
            [CompletionResult]::new('--index-large-file-policy', '--index-large-file-policy', [CompletionResultType]::ParameterName, 'Optional large file policy for this doctor run')
            [CompletionResult]::new('--index-execution', '--index-execution', [CompletionResultType]::ParameterName, 'Optional index execution model for this doctor run')
            [CompletionResult]::new('--index-chunk-size', '--index-chunk-size', [CompletionResultType]::ParameterName, 'Optional chunk size for multiprocess mode')
            [CompletionResult]::new('--strict', '--strict', [CompletionResultType]::ParameterName, 'Fail with non-zero status when warnings/findings are present')
            [CompletionResult]::new('--parity-rerun', '--parity-rerun', [CompletionResultType]::ParameterName, 'Re-run the index build twice and ensure symbol counts and hashes match exactly')
            [CompletionResult]::new('--compare-with-full', '--compare-with-full', [CompletionResultType]::ParameterName, 'Compare index contents against a ''full'' mode run')
            [CompletionResult]::new('--profile-index', '--profile-index', [CompletionResultType]::ParameterName, 'Run indexing multiple times to generate a performance profile')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;h' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help')
            break
        }
        'curd;help' {
            [CompletionResult]::new('mcp', 'mcp', [CompletionResultType]::ParameterValue, 'Start the Model Context Protocol (MCP) server over stdin/stdout')
            [CompletionResult]::new('init-agent', 'init-agent', [CompletionResultType]::ParameterValue, 'Initialize and authorize a new agent keypair, auto-configuring a specified harness')
            [CompletionResult]::new('doctor', 'doctor', [CompletionResultType]::ParameterValue, 'Run built-in self diagnostics for indexing and regressions')
            [CompletionResult]::new('build', 'build', [CompletionResultType]::ParameterValue, 'Build via CURD control plane (adapter-based dry-run/execute)')
            [CompletionResult]::new('hook', 'hook', [CompletionResultType]::ParameterValue, 'Print a shell hook to implicitly route standard commands (like ''make'') through CURD')
            [CompletionResult]::new('diff', 'diff', [CompletionResultType]::ParameterValue, 'Compare symbols at AST level')
            [CompletionResult]::new('refactor', 'refactor', [CompletionResultType]::ParameterValue, 'Semantic refactoring engine')
            [CompletionResult]::new('context', 'context', [CompletionResultType]::ParameterValue, 'Manage external workspace contexts (Read-Only or Semantic linking)')
            [CompletionResult]::new('init', 'init', [CompletionResultType]::ParameterValue, 'Initialize CURD workspace: auto-detect build system, create .curd/ directory')
            [CompletionResult]::new('config', 'config', [CompletionResultType]::ParameterValue, 'Manage CURD configuration and policies')
            [CompletionResult]::new('plugin-language', 'plugin-language', [CompletionResultType]::ParameterValue, 'Install, remove, or list signed language plugins (.curdl)')
            [CompletionResult]::new('plugin-tool', 'plugin-tool', [CompletionResultType]::ParameterValue, 'Install, remove, or list signed tool plugins (.curdt)')
            [CompletionResult]::new('plugin-trust', 'plugin-trust', [CompletionResultType]::ParameterValue, 'Manage trusted signing keys for CURD plugin packages')
            [CompletionResult]::new('detach', 'detach', [CompletionResultType]::ParameterValue, 'Soft detach CURD from the current workspace (removes git hooks and scrubs scripts)')
            [CompletionResult]::new('delete', 'delete', [CompletionResultType]::ParameterValue, 'Permanently delete CURD from the current workspace by removing the .curd/ directory')
            [CompletionResult]::new('status', 'status', [CompletionResultType]::ParameterValue, 'Print a summary of the current workspace state (index stats, shadow changes, etc.)')
            [CompletionResult]::new('log', 'log', [CompletionResultType]::ParameterValue, 'Tail the agent''s mutation and execution history')
            [CompletionResult]::new('repl', 'repl', [CompletionResultType]::ParameterValue, 'Start the interactive CURD REPL for semantic exploration')
            [CompletionResult]::new('run', 'run', [CompletionResultType]::ParameterValue, 'Run a .curd script by compiling it to the current DSL IR')
            [CompletionResult]::new('test', 'test', [CompletionResultType]::ParameterValue, 'Run semantic integrity tests on the symbol graph')
            [CompletionResult]::new('search', 'search', [CompletionResultType]::ParameterValue, 'Semantic search across the indexed graph')
            [CompletionResult]::new('graph', 'graph', [CompletionResultType]::ParameterValue, 'Explore the caller/callee dependency graph of a symbol')
            [CompletionResult]::new('read', 'read', [CompletionResultType]::ParameterValue, 'Read a file or semantic symbol exactly as the engine parses it')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Manually mutate a file or symbol via the AST-aware EditEngine')
            [CompletionResult]::new('diagram', 'diagram', [CompletionResultType]::ParameterValue, 'Generate diagrams from the symbol graph')
            [CompletionResult]::new('session', 'session', [CompletionResultType]::ParameterValue, 'Manage manual ShadowStore transaction sessions')
            [CompletionResult]::new('plan', 'plan', [CompletionResultType]::ParameterValue, 'Manage, inspect, and execute saved AI plans')
            [CompletionResult]::new('index-worker', 'index-worker', [CompletionResultType]::ParameterValue, 'index-worker')
            [CompletionResult]::new('completions', 'completions', [CompletionResultType]::ParameterValue, 'Generate shell completion scripts')
            [CompletionResult]::new('man', 'man', [CompletionResultType]::ParameterValue, 'Generate man pages')
            [CompletionResult]::new('index', 'index', [CompletionResultType]::ParameterValue, 'Alias for curd doctor indexing')
            [CompletionResult]::new('h', 'h', [CompletionResultType]::ParameterValue, 'Show help for CURD')
            [CompletionResult]::new('help', 'help', [CompletionResultType]::ParameterValue, 'Print this message or the help of the given subcommand(s)')
            break
        }
        'curd;help;mcp' {
            break
        }
        'curd;help;init-agent' {
            break
        }
        'curd;help;doctor' {
            break
        }
        'curd;help;build' {
            break
        }
        'curd;help;hook' {
            break
        }
        'curd;help;diff' {
            break
        }
        'curd;help;refactor' {
            [CompletionResult]::new('rename', 'rename', [CompletionResultType]::ParameterValue, 'Rename a function/class/variable')
            [CompletionResult]::new('move', 'move', [CompletionResultType]::ParameterValue, 'Move a function between files')
            [CompletionResult]::new('extract', 'extract', [CompletionResultType]::ParameterValue, 'Extract code into a new function')
            break
        }
        'curd;help;refactor;rename' {
            break
        }
        'curd;help;refactor;move' {
            break
        }
        'curd;help;refactor;extract' {
            break
        }
        'curd;help;context' {
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add an external repository context')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a linked context by alias or path')
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List all currently linked contexts')
            break
        }
        'curd;help;context;add' {
            break
        }
        'curd;help;context;remove' {
            break
        }
        'curd;help;context;list' {
            break
        }
        'curd;help;init' {
            break
        }
        'curd;help;config' {
            [CompletionResult]::new('show', 'show', [CompletionResultType]::ParameterValue, 'Show current workspace configuration')
            [CompletionResult]::new('set', 'set', [CompletionResultType]::ParameterValue, 'Set a configuration value (e.g. `set index.mode fast`)')
            [CompletionResult]::new('unset', 'unset', [CompletionResultType]::ParameterValue, 'Remove a configuration value')
            break
        }
        'curd;help;config;show' {
            break
        }
        'curd;help;config;set' {
            break
        }
        'curd;help;config;unset' {
            break
        }
        'curd;help;plugin-language' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List installed plugin packages')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Install a signed plugin archive')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove an installed plugin package')
            break
        }
        'curd;help;plugin-language;list' {
            break
        }
        'curd;help;plugin-language;add' {
            break
        }
        'curd;help;plugin-language;remove' {
            break
        }
        'curd;help;plugin-tool' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List installed plugin packages')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Install a signed plugin archive')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove an installed plugin package')
            break
        }
        'curd;help;plugin-tool;list' {
            break
        }
        'curd;help;plugin-tool;add' {
            break
        }
        'curd;help;plugin-tool;remove' {
            break
        }
        'curd;help;plugin-trust' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List trusted signing keys')
            [CompletionResult]::new('get', 'get', [CompletionResultType]::ParameterValue, 'Get one trusted signing key')
            [CompletionResult]::new('add', 'add', [CompletionResultType]::ParameterValue, 'Add or replace a trusted signing key')
            [CompletionResult]::new('remove', 'remove', [CompletionResultType]::ParameterValue, 'Remove a trusted signing key')
            [CompletionResult]::new('enable', 'enable', [CompletionResultType]::ParameterValue, 'Enable a trusted signing key')
            [CompletionResult]::new('disable', 'disable', [CompletionResultType]::ParameterValue, 'Disable a trusted signing key')
            break
        }
        'curd;help;plugin-trust;list' {
            break
        }
        'curd;help;plugin-trust;get' {
            break
        }
        'curd;help;plugin-trust;add' {
            break
        }
        'curd;help;plugin-trust;remove' {
            break
        }
        'curd;help;plugin-trust;enable' {
            break
        }
        'curd;help;plugin-trust;disable' {
            break
        }
        'curd;help;detach' {
            break
        }
        'curd;help;delete' {
            break
        }
        'curd;help;status' {
            break
        }
        'curd;help;log' {
            break
        }
        'curd;help;repl' {
            break
        }
        'curd;help;run' {
            break
        }
        'curd;help;test' {
            break
        }
        'curd;help;search' {
            break
        }
        'curd;help;graph' {
            break
        }
        'curd;help;read' {
            break
        }
        'curd;help;edit' {
            break
        }
        'curd;help;diagram' {
            break
        }
        'curd;help;session' {
            [CompletionResult]::new('begin', 'begin', [CompletionResultType]::ParameterValue, 'Open a new shadow transaction')
            [CompletionResult]::new('review', 'review', [CompletionResultType]::ParameterValue, 'View changes and architectural impact of the current session')
            [CompletionResult]::new('log', 'log', [CompletionResultType]::ParameterValue, 'View a detailed log of all tool calls and their results')
            [CompletionResult]::new('commit', 'commit', [CompletionResultType]::ParameterValue, 'Commit the active shadow transaction to disk')
            [CompletionResult]::new('rollback', 'rollback', [CompletionResultType]::ParameterValue, 'Discard the active shadow transaction')
            break
        }
        'curd;help;session;begin' {
            break
        }
        'curd;help;session;review' {
            break
        }
        'curd;help;session;log' {
            break
        }
        'curd;help;session;commit' {
            break
        }
        'curd;help;session;rollback' {
            break
        }
        'curd;help;plan' {
            [CompletionResult]::new('list', 'list', [CompletionResultType]::ParameterValue, 'List all saved plans in the workspace')
            [CompletionResult]::new('read', 'read', [CompletionResultType]::ParameterValue, 'Read a specific plan and format it as a readable execution tree')
            [CompletionResult]::new('edit', 'edit', [CompletionResultType]::ParameterValue, 'Edit a saved plan or compiled script artifact interactively')
            [CompletionResult]::new('impl', 'impl', [CompletionResultType]::ParameterValue, 'Execute a saved plan')
            break
        }
        'curd;help;plan;list' {
            break
        }
        'curd;help;plan;read' {
            break
        }
        'curd;help;plan;edit' {
            break
        }
        'curd;help;plan;impl' {
            break
        }
        'curd;help;index-worker' {
            break
        }
        'curd;help;completions' {
            break
        }
        'curd;help;man' {
            break
        }
        'curd;help;index' {
            break
        }
        'curd;help;h' {
            break
        }
        'curd;help;help' {
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}

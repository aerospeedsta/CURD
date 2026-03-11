module.exports = grammar({
  name: "curd",

  extras: ($) => [/\s/, $.comment],

  word: ($) => $.identifier,

  rules: {
    source_file: ($) => repeat($._statement),

    _statement: ($) =>
      choice(
        $.arg_decl,
        $.let_decl,
        $.tool_call,
        $.sequence_block,
        $.parallel_block,
        $.atomic_block,
        $.abort_stmt
      ),

    comment: () => token(seq("#", /.*/)),

    arg_decl: ($) =>
      seq(
        "arg",
        field("name", $.identifier),
        optional(seq(":", field("type", $.identifier))),
        optional(seq("=", field("value", $._value)))
      ),

    let_decl: ($) =>
      seq("let", field("name", $.identifier), "=", field("value", $._value)),

    tool_call: ($) =>
      seq(field("tool", $.identifier), repeat1(choice($.named_argument, $._value))),

    named_argument: ($) =>
      seq(field("name", $.identifier), "=", field("value", $._value)),

    sequence_block: ($) =>
      seq("sequence", optional(field("name", $.identifier)), $.block),

    parallel_block: ($) =>
      seq("parallel", optional(field("name", $.identifier)), $.block),

    atomic_block: ($) =>
      seq("atomic", optional(field("name", $.identifier)), $.block),

    abort_stmt: ($) => seq("abort", field("reason", $._value)),

    block: ($) => seq("{", repeat($._statement), "}"),

    _value: ($) =>
      choice(
        $.multiline_string,
        $.string,
        $.number,
        $.boolean,
        $.variable_ref,
        $.array,
        $.object,
        $.identifier
      ),

    variable_ref: ($) => seq("$", $.identifier),

    array: ($) => seq("[", optional(seq($._value, repeat(seq(",", $._value)))), "]"),

    object: ($) =>
      seq(
        "{",
        optional(seq($.pair, repeat(seq(",", $.pair)))),
        "}"
      ),

    pair: ($) => seq(field("key", $.identifier), ":", field("value", $._value)),

    boolean: () => choice("true", "false"),

    number: () => /\d+/,

    multiline_string: () =>
      token(seq('"""', repeat(choice(/[^"]+/, /"[^"]/ , /""[^"]/)), '"""')),

    string: () => token(seq('"', repeat(choice(/[^"\\]+/, /\\./)), '"')),

    identifier: () => /[A-Za-z_][A-Za-z0-9_.-]*/
  }
});

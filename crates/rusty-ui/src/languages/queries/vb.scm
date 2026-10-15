(comment) @comment

[
  (string_literal)
  (character_literal)
  (interpolated_string_literal)
] @string

[
  (integer_literal)
  (floating_point_literal)
] @number

(boolean_literal) @boolean
(date_literal) @constant
(modifier) @keyword
(primitive_type) @type
(preprocessor_directive) @preproc

(class_block name: (identifier) @type)
(module_block name: (identifier) @type)
(structure_block name: (identifier) @type)
(interface_block name: (identifier) @type)
(enum_block name: (identifier) @type)
(namespace_block name: (namespace_name) @type)
(imports_statement namespace: (namespace_name) @type)
(as_clause type: (type) @type)

(method_declaration name: (identifier) @function)
(property_declaration name: (identifier) @property)
(invocation target: (identifier) @function)
(attribute name: (_) @attribute)

[ "(" ")" "{" "}" ] @punctuation.bracket
[ "," "." ":" ] @punctuation.delimiter
[ "=" "<>" "<" ">" "<=" ">=" "+" "-" "*" "/" "\\" "^" "&=" "+=" "-=" ] @operator

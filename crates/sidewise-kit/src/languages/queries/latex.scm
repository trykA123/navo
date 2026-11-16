[
  (line_comment)
  (block_comment)
  (comment_environment)
] @comment

(command_name) @function

(begin
  command: _ @keyword
  name: (curly_group_text (text) @type))

(end
  command: _ @keyword
  name: (curly_group_text (text) @type))

(class_include command: _ @keyword)
(package_include command: _ @keyword)
(new_command_definition command: _ @keyword)
(label_definition command: _ @keyword)
(label_reference command: _ @keyword)
(citation command: _ @keyword)

(class_include path: (curly_group_path) @string)
(package_include paths: (curly_group_path_list) @string)
(label_definition name: (curly_group_label) @label)
(label_reference names: (curly_group_label_list) @label)
(citation keys: (curly_group_text_list) @label)

(chapter command: _ @keyword text: (curly_group) @title)
(section command: _ @keyword text: (curly_group) @title)
(subsection command: _ @keyword text: (curly_group) @title)

(key_value_pair key: (text) @property)

[
  (inline_formula)
  (displayed_equation)
] @string.special

[ "{" "}" "[" "]" ] @punctuation.bracket

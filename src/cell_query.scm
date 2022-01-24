(module (function_definition name: (identifier) @function parameters: (parameters) body: (block)))
(module (expression_statement (assignment left: (identifier) @variable)))
(module (import_statement name: (dotted_name (identifier) @import)))
(module (import_statement name: (aliased_import name: (dotted_name (identifier)) alias: (identifier) @import)))
(identifier) @identifier
(ERROR) @error
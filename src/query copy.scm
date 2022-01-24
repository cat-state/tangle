(module 
  (function_definition name: (identifier) @leaf.name
    parameters: (parameters (identifier)*  @leaf.param) 
    body: 
    (block 
      (return_statement 
        (expression_list (identifier)* @leaf.provides))) @leaf.body)) @leaf

(module 
  (function_definition name: (identifier) @flow.name
    parameters: (parameters (identifier)* @flow.param)
    body: 
    (block 
      (expression_statement
        (assignment left: (left_hand_side [
           (list_pattern (identifier)+ @flow.node.provides)
           (identifier) @flow.node.provides
          ])
          right: (expression_list 
            (call function: (identifier) @flow.node.name
              arguments: (argument_list) @flow.node.param))) @flow.node)*
      (return_statement (expression_list (identifier)* @flow.provides))))) @flow

(module 
  (expression_statement 
    (assignment left: (left_hand_side (identifier) @variable))))
(module 
  (import_statement name: (dotted_name (identifier) @import)))
(module 
  (import_statement name: (aliased_import name: (dotted_name (identifier)) alias: (identifier) @import)))


(identifier) @identifier
(ERROR) @error

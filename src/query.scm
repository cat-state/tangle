(module 
  (function_definition name: (identifier) @leaf.name
    parameters: (parameters (identifier)*) @leaf.param
    body: 
    (block 
      [(_)* 
       (expression_statement 
        (assignment left: [(list_pattern) (identifier)] @flow.node.provides 
          right: (call function: (identifier) @flow.node.name arguments: (argument_list) @flow.node.param)) @flow.node)*] @leaf.body
      (return_statement [(identifier) (list (identifier)+)] @leaf.provides))) @leaf)
; (module 
;   (function_definition name: (identifier) @leaf.name
;     parameters: (parameters (identifier)*) @leaf.param
;     body: 
;     (block
;       (return_statement [(identifier) (list (identifier)+)] @flow.provides))))
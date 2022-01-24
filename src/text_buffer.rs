// use tree_sitter::Tree;
// pub struct TextBuffer {
//     pub code: String,
//     pub tree: Option<tree_sitter::Tree>
// }

// impl AsRef<str> for TextBuffer {
//     fn as_ref(&self) -> &str {
//         self.code.as_ref()
//     }
// }

// impl egui::TextBuffer for TextBuffer {
//     fn insert_text(&mut self, text: &str, char_index: usize) -> usize {
//         if text.contains("\n") {
//             self.tree = None;
//         }

//         if let Some(tree) = self.tree {
//             let (ith, lst_line) = self.code[..char_index].lines().enumerate().last().unwrap_or((0, ""));
//             tree.edit(&tree_sitter::InputEdit {
//                 start_byte: char_index,
//                 old_end_byte: char_index,
//                 new_end_byte: char_index + text.len(),
//                 start_position: tree_sitter::Point { row: ith, column: lst_line.len() },
//                 old_end_position: tree_sitter::Point { row: ith, column: lst_line.len() },
//                 new_end_position: tree_sitter::Point { row: ith, column: lst_line.len() + text.len() }
//             });
//         }
//         self.code.insert_text(text, char_index)
//     }

//     /// Deletes a range of text `char_range` from this buffer.
//     ///
//     /// # Notes
//     /// `char_range` is a *character range*, not a byte range.
//     fn delete_char_range(&mut self, char_range: std::ops::Range<usize>) {
//         if self.code[char_range].contains("\n") {
//             self.tree = None;
//         }
        
//         if let Some(tree) = self.tree {
//             let (ith, lst_line) = self.code[..char_range.end].lines().enumerate().last().unwrap_or((0, ""));
//             tree.edit(&tree_sitter::InputEdit {
//                 start_byte: char_range.start,
//                 old_end_byte: char_range.end,
//                 new_end_byte: char_range.start,
//                 start_position: tree_sitter::Point { row: ith, column: lst_line.len() },
//                 old_end_position: tree_sitter::Point { row: ith, column: lst_line.len() },
//                 new_end_position: tree_sitter::Point { row: ith, column: 0 }
//             });
//         }
//         self.code.delete_char_range(char_range)
//     }

//     fn is_mutable(&self) -> bool {
//         true
//     }
// }
use crate::colors::hex_to_rgb;
use crossterm::style::{Color, Stylize};
use devicons::{Theme, icon_for_file};
use std::collections::BTreeMap;

#[derive(Debug)]
struct TreeNode {
    is_file: bool,
    children: BTreeMap<String, TreeNode>,
}

impl TreeNode {
    fn new(is_file: bool) -> Self {
        Self {
            is_file,
            children: BTreeMap::new(),
        }
    }
}

pub fn get_file_icons(files: &[String], theme: &Option<Theme>) -> String {
    if files.is_empty() {
        return String::new();
    }

    // Build the tree structure
    let mut root = TreeNode::new(false);

    for file_path in files {
        let parts: Vec<&str> = file_path.split('/').collect();
        let mut current = &mut root;

        for (i, part) in parts.iter().enumerate() {
            let is_file = i == parts.len() - 1;
            current
                .children
                .entry(part.to_string())
                .or_insert_with(|| TreeNode::new(is_file));
            current = current.children.get_mut(*part).unwrap();
        }
    }

    // Flatten single-child directories
    flatten_tree(&mut root);

    // Generate the tree display
    let mut result = Vec::new();
    print_tree(&root, theme, "", true, &mut result);

    result.join("\n")
}

fn flatten_tree(node: &mut TreeNode) {
    let mut to_flatten = Vec::new();

    for (name, child) in &mut node.children {
        flatten_tree(child);

        // If this directory has only one child and it's also a directory, flatten it
        if !child.is_file && child.children.len() == 1 {
            let (child_name, _grandchild) = child.children.iter().next().unwrap();
            to_flatten.push((name.clone(), child_name.clone()));
        }
    }

    for (dir_name, child_name) in to_flatten {
        if let Some(dir_node) = node.children.remove(&dir_name) {
            if let Some((_, grandchild)) = dir_node.children.into_iter().next() {
                let flattened_name = format!("{dir_name}/{child_name}");
                node.children.insert(flattened_name, grandchild);
            }
        }
    }
}

fn print_tree(
    node: &TreeNode,
    theme: &Option<Theme>,
    prefix: &str,
    _is_last: bool,
    result: &mut Vec<String>,
) {
    for (i, (name, child)) in node.children.iter().enumerate() {
        let is_child_last = i == node.children.len() - 1;
        let current_prefix = if prefix.is_empty() { "    " } else { prefix };

        let tree_char = if is_child_last {
            "└── "
        } else {
            "├── "
        };

        let (icon_str, icon_color) = if child.is_file {
            let icon = icon_for_file(name, theme);
            (icon.icon.to_string(), hex_to_rgb(icon.color))
        } else {
            ("\u{e5ff}".to_string(), Color::White) // Color will be overridden below
        };

        let gray_color = Color::Rgb {
            r: 128,
            g: 128,
            b: 128,
        };

        let (name_part, icon_part) = if child.is_file {
            // For files, split the path and color directory parts gray, filename white
            let name_formatted = if let Some(last_slash) = name.rfind('/') {
                let dir_part = &name[..last_slash + 1];
                let file_part = &name[last_slash + 1..];
                format!(
                    "{}{}",
                    dir_part.with(gray_color),
                    file_part.with(Color::White)
                )
            } else {
                name.as_str().with(Color::White).to_string()
            };
            (name_formatted, format!(" {}", icon_str.with(icon_color)))
        } else {
            // For directories, no icon - just the name in gray
            (name.as_str().with(gray_color).to_string(), String::new())
        };

        let line = format!(
            "{}{} {}{}",
            current_prefix,
            tree_char.with(gray_color),
            name_part,
            icon_part
        );

        result.push(line);

        if !child.children.is_empty() {
            let next_prefix = format!(
                "{}{}    ",
                current_prefix,
                if is_child_last {
                    " ".to_string()
                } else {
                    "│".with(gray_color).to_string()
                }
            );
            print_tree(child, theme, &next_prefix, false, result);
        }
    }
}

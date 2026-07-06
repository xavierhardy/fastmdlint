//! MD042 — no-empty-links.
//!
//! Note: reference-definition `#` detection (which needs the definition map)
//! is not modelled; empty inline resources (`[x]()`, `[x](#)`) are detected.

use super::{Emit, Params, RuleMeta};

pub const RULE: RuleMeta = RuleMeta {
    names: &["MD042", "no-empty-links"],
    description: "No empty links",
    tags: &["links"],
    micromark: true,
    run,
};

fn run(params: &Params, emit: &mut Emit) {
    let tree = params.tree;
    for &link in &tree.filter_idx(&["link"]) {
        let label_text = tree.descendants_by_type(link, &[&["label"], &["labelText"]]);
        let reference = tree.descendants_by_type(link, &[&["reference"]]);
        let resource = tree.descendants_by_type(link, &[&["resource"]]);
        let reference_string = if let Some(&r) = reference.first() {
            tree.descendants_by_type(r, &[&["referenceString"]])
        } else {
            vec![]
        };
        let resource_dest_string = if let Some(&r) = resource.first() {
            tree.descendants_by_type(
                r,
                &[
                    &["resourceDestination"],
                    &["resourceDestinationLiteral", "resourceDestinationRaw"],
                    &["resourceDestinationString"],
                ],
            )
        } else {
            vec![]
        };
        let has_label_text = !label_text.is_empty();
        let has_reference = !reference.is_empty();
        let has_resource = !resource.is_empty();
        let has_reference_string = !reference_string.is_empty();
        let has_resource_dest_string = !resource_dest_string.is_empty();

        let error = if has_label_text
            && ((!has_reference && !has_resource) || (has_reference && !has_reference_string))
        {
            false // would need definitions to know if it points to "#"
        } else if has_reference_string && !has_resource_dest_string {
            false
        } else if !has_reference_string && has_resource_dest_string {
            tree.get(resource_dest_string[0]).text.trim() == "#"
        } else {
            !has_reference_string && !has_resource_dest_string
        };

        if error {
            let t = tree.get(link);
            emit.add_context(
                t.start_line,
                &t.text,
                false,
                false,
                Some((t.start_column, t.end_column - t.start_column)),
                None,
            );
        }
    }
}

use primitives::{primitive::{TagGroup, Vector}, tag::PrimaryTagStructDyn};
use ringhopper_structs::{GBXModel, Model};

use crate::tag::model::ModelFunctions;
use super::BludgeonResult;

pub fn repair_model(tag: &mut dyn PrimaryTagStructDyn) -> BludgeonResult {
    let model_fns: &mut dyn ModelFunctions = match tag.group() {
        TagGroup::Model => tag.as_any_mut().downcast_mut::<Model>().unwrap(),
        TagGroup::GBXModel => tag.as_any_mut().downcast_mut::<GBXModel>().unwrap(),
        g => unreachable!("can't repair non-model group {g}")
    };

    if model_fns.fix_runtime_markers().is_err() {
        return BludgeonResult::CannotRepair
    }

    model_fns.fix_compressed_vertices();
    model_fns.fix_uncompressed_vertices();

    for node in model_fns.nodes_mut() {
        node.default_rotation = node.default_rotation.normalize();
    }
    for region in model_fns.regions_mut() {
        for permutation in &mut region.permutations {
            for marker in &mut permutation.markers {
                marker.rotation = marker.rotation.normalize();
            }
        }
    }

    BludgeonResult::Done
}

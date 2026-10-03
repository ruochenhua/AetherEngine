//! Owned glTF document decoding and T7.1 derived animation assets.

mod derived;
mod parse;
mod types;

#[cfg(test)]
mod tests;

pub use derived::{AnimationClipAsset, Joint, SkeletonAsset};
pub use parse::load_document;
pub use types::{
    AnimationChannel, AnimationValues, ChannelTarget, GltfAnimation, GltfAnimationChannel,
    GltfDocumentAsset, GltfError, GltfImage, GltfImageFormat, GltfMesh, GltfNode, GltfPrimitive,
    GltfSkin, Interpolation,
};

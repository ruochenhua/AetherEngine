// IBL generation helpers.
//
// This module is a facade over the specialized IBL submodules so existing
// callers can keep using `ibl::generate::*` without changing their imports.

pub(super) use super::cubemap::{create_cubemap, CpuCubemap, CubeMesh};
pub(super) use super::hdr::load_hdr_texture;

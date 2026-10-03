use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(1);

pub struct Fixture {
    pub root: PathBuf,
    pub path: PathBuf,
}

impl Fixture {
    pub fn new(joint_count: usize, cycle: bool, include_other_mesh: bool) -> Self {
        let id = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "aether_t7_acceptance_{}_{}",
            std::process::id(),
            id
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("actor.gltf");

        let mut binary = Vec::new();
        for value in [0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            push_f32(&mut binary, value);
        }
        for _ in 0..12 {
            binary.extend_from_slice(&0_u16.to_le_bytes());
        }
        for _ in 0..3 {
            for value in [1.0_f32, 0.0, 0.0, 0.0] {
                push_f32(&mut binary, value);
            }
        }
        fs::write(root.join("actor.bin"), &binary).unwrap();

        let mut nodes = vec![json!({"name":"Mesh","mesh":0,"skin":0})];
        for joint_index in 0..joint_count {
            let node_index = joint_index + 1;
            let mut node = json!({"name":format!("Joint{joint_index}")});
            if node_index < joint_count {
                node["children"] = json!([node_index + 1]);
            }
            nodes.push(node);
        }
        if cycle && joint_count >= 2 {
            nodes[1]["children"] = json!([2]);
            nodes[2]["children"] = json!([1]);
        }
        let other_node = nodes.len();
        if include_other_mesh {
            nodes.push(json!({"name":"OtherStatic","mesh":1}));
        }

        let mut meshes = vec![
            json!({"name":"SkinnedTriangle","primitives":[{"attributes":{
                "POSITION":0,"JOINTS_0":1,"WEIGHTS_0":2
            }}]}),
        ];
        if include_other_mesh {
            meshes
                .push(json!({"name":"OtherTriangle","primitives":[{"attributes":{"POSITION":0}}]}));
        }
        let mut scene_nodes = vec![0, 1];
        if include_other_mesh {
            scene_nodes.push(other_node);
        }
        let document: Value = json!({
            "asset":{"version":"2.0"},
            "scene":0,
            "scenes":[{"nodes":scene_nodes}],
            "nodes":nodes,
            "meshes":meshes,
            "skins":[{"name":"TestSkin","joints":(1..=joint_count).collect::<Vec<_>>(),"skeleton":1}],
            "buffers":[{"uri":"actor.bin","byteLength":binary.len()}],
            "bufferViews":[
                {"buffer":0,"byteOffset":0,"byteLength":36},
                {"buffer":0,"byteOffset":36,"byteLength":24},
                {"buffer":0,"byteOffset":60,"byteLength":48}
            ],
            "accessors":[
                {"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]},
                {"bufferView":1,"componentType":5123,"count":3,"type":"VEC4"},
                {"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}
            ]
        });
        fs::write(&path, serde_json::to_vec_pretty(&document).unwrap()).unwrap();
        Self { root, path }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn push_f32(bytes: &mut Vec<u8>, value: f32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

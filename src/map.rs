use std::collections::HashMap;

use glam::I16Vec3;
use luanti_core::{MapBlockNodes, MapBlockPos, MapNode, MapNodePos};

pub struct LuantiMap {
    blocks: HashMap<MapBlockPos, MapBlockNodes>,
}

impl LuantiMap {
    /// Returns an empty map.
    pub fn new() -> Self {
        Self {
            blocks: HashMap::new(),
        }
    }

    /// Inserts a mapblock into the map.
    pub fn insert_block(&mut self, blockpos: MapBlockPos, data: MapBlockNodes) {
        self.blocks.insert(blockpos, data);
    }

    /// Gets a mapblock from the map.
    pub fn get_block(&self, blockpos: &MapBlockPos) -> Option<&MapBlockNodes> {
        self.blocks.get(blockpos)
    }

    /// Sets a node in the map.
    /// No-op if the mapblock that would contain the node does not exist.
    pub fn set_node(&mut self, pos: &MapNodePos, node: MapNode) {
        let (blockpos, index) = pos.split_index();

        if let Some(block) = self.blocks.get_mut(&blockpos) {
            block[index] = node;
        }
    }
}

const NEIGHBOR_DIRS: [I16Vec3; 6] = [
    I16Vec3::Y,
    I16Vec3::NEG_Y,
    I16Vec3::X,
    I16Vec3::NEG_X,
    I16Vec3::Z,
    I16Vec3::NEG_Z,
];

/// Stores a clone of a mapblock and its 6 neighbors (if those exist).
/// Used for sending map data to meshgen.
pub struct MeshgenMapData {
    block: MapBlockNodes,
    // Order: see NEIGHBOR_DIRS
    neighbors: [Option<MapBlockNodes>; 6],
}

impl MeshgenMapData {
    pub fn new(map: &LuantiMap, blockpos: MapBlockPos) -> Option<Self> {
        let block = map.get_block(&blockpos)?;
        let mut result = Self {
            block: block.clone(),
            neighbors: [const { None }; 6],
        };

        for (index, dir) in NEIGHBOR_DIRS.into_iter().enumerate() {
            let Some(n_blockpos) = blockpos.checked_add(dir) else {
                continue;
            };
            let Some(n_block) = map.get_block(&n_blockpos) else {
                continue;
            };
            result.neighbors[index] = Some(n_block.clone());
        }

        Some(result)
    }

    /// Returns a node from this mapblock or its neighbors.
    /// Coordinates are relative to the main mapblock.
    pub fn get_node(&self, pos: MapNodePos) -> Option<MapNode> {
        let (blockpos, index) = pos.split_index();

        let vec = blockpos.vec();

        if vec == I16Vec3::ZERO {
            return Some(self.block[index]);
        }

        // The inverse of NEIGHBOR_DIRS
        let n_index = match vec {
            pos if pos == I16Vec3::Y => Some(0),
            pos if pos == I16Vec3::NEG_Y => Some(1),
            pos if pos == I16Vec3::X => Some(2),
            pos if pos == I16Vec3::NEG_X => Some(3),
            pos if pos == I16Vec3::Z => Some(4),
            pos if pos == I16Vec3::NEG_Z => Some(5),
            _ => None,
        };
        if let Some(n_index) = n_index {
            return self.neighbors[n_index]
                .as_ref()
                .and_then(|block| Some(block[index]));
        };

        None
    }
}

use std::collections::HashMap;

use luanti_core::ContentId;
use luanti_protocol::types::{ContentFeatures, DrawType, ParamType, TileDef};

pub struct NodeDefManager {
    // TODO: should be private
    pub map: HashMap<ContentId, ContentFeatures>,
}

impl NodeDefManager {
    /// Creates a new NodeDefManager from luanti_protocol data.
    pub fn from_network(data: luanti_protocol::types::NodeDefManager) -> Self {
        let mut map = HashMap::new();
        // These three are not sent via the network by Luanti, we are expected
        // to initialize them ourselves.
        map.insert(
            ContentId::UNKNOWN,
            ContentFeatures {
                name: String::from("unknown"),
                tiledef: std::array::from_fn(|_| TileDef {
                    name: String::from("unknown_node.png"),
                    ..TileDef::default()
                }),
                ..ContentFeatures::default()
            },
        );
        map.insert(
            ContentId::AIR,
            ContentFeatures {
                name: String::from("air"),
                drawtype: DrawType::AirLike,
                param_type: ParamType::Light,
                light_propagates: true,
                sunlight_propagates: true,
                walkable: false,
                pointable: false,
                diggable: false,
                buildable_to: true,
                floodable: true,
                is_ground_content: true,
                ..ContentFeatures::default()
            },
        );
        map.insert(
            ContentId::IGNORE,
            ContentFeatures {
                name: String::from("ignore"),
                drawtype: DrawType::AirLike,
                param_type: ParamType::None,
                light_propagates: false,
                sunlight_propagates: false,
                walkable: false,
                pointable: false,
                diggable: false,
                buildable_to: true,
                is_ground_content: true,

                ..ContentFeatures::default()
            },
        );
        for (id, def) in data.content_features {
            map.insert(ContentId(id), def);
        }
        Self { map }
    }

    pub fn get(&self, content_id: ContentId) -> Option<&ContentFeatures> {
        self.map.get(&content_id)
    }

    pub fn get_with_fallback(&self, content_id: ContentId) -> &ContentFeatures {
        self.get(content_id)
            .unwrap_or_else(|| self.map.get(&ContentId::UNKNOWN).unwrap())
    }
}

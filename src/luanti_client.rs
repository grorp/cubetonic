use std::f32::consts::PI;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::anyhow;
use glam::Vec3;
use luanti_core::{ContentId, MapBlockNodes, MapBlockPos, MapNode, MapNodePos};
use luanti_protocol::LuantiClient;
use luanti_protocol::commands::client_to_server::{
    ClientReadySpec, FirstSrpSpec, GotBlocksSpec, Init2Spec, InitSpec, PlayerPosCommand,
    ToServerCommand,
};
use luanti_protocol::commands::server_to_client::ToClientCommand;
use rand::Rng;
use tokio::sync::mpsc;

use crate::map::{LuantiMap, NEIGHBOR_DIRS};
use crate::meshgen::{MapblockMesh, MeshgenTask};

// Luanti's "BS" factor
const BS: f32 = 10.0;

pub enum MainToClientEvent {
    PlayerPos { pos: Vec3, yaw: f32, pitch: f32 },
}

pub struct LuantiClientRunner {
    device: Arc<wgpu::Device>,
    main_rx: mpsc::UnboundedReceiver<MainToClientEvent>,
    meshgen_tx: mpsc::UnboundedSender<MapblockMesh>,

    client: LuantiClient,
    map: LuantiMap,
}

impl LuantiClientRunner {
    pub async fn spawn(
        device: Arc<wgpu::Device>,
        main_rx: mpsc::UnboundedReceiver<MainToClientEvent>,
        meshgen_tx: mpsc::UnboundedSender<MapblockMesh>,
    ) {
        tokio::spawn(async move {
            let addr: SocketAddr = "127.0.0.1:3000".parse().unwrap();
            println!("Connecting to Luanti server at {}...", addr);
            let client = LuantiClient::connect(addr).await.unwrap();

            let map = LuantiMap::new();

            let mut runner = LuantiClientRunner {
                device,
                main_rx,
                meshgen_tx,

                client,
                map,
            };
            runner.run().await
        });
    }

    async fn run(&mut self) {
        match self.run_inner().await {
            Ok(()) => unreachable!(),
            Err(err) => {
                println!("Disconnected: {}", err);
            }
        }
    }

    async fn run_inner(&mut self) -> anyhow::Result<()> {
        let mut user_name = String::from("test");
        user_name.push_str(&rand::rng().random_range(0..1000).to_string());

        self.client.send(ToServerCommand::Init(Box::new(InitSpec {
            serialization_ver_max: 29,
            supp_compr_modes: 0, // unused
            min_net_proto_version: 46,
            max_net_proto_version: 46, // appears to be the only version supported by luanti-protocol
            user_name: user_name.clone(),
        })))?;

        loop {
            // println!("Waiting for command...");

            tokio::select! {
                command = self.client.recv() => {
                    // println!("Received command from server: {:?}", command);
                    let command = command?;
                    self.process_network_command(command)?;
                },

                event = self.main_rx.recv() => {
                    let event = event.ok_or_else(|| anyhow!("main_rx is closed"))?;
                    self.process_main_event(event)?;
                },
            }
        }
    }

    fn generate_mapblock(&self, blockpos: MapBlockPos, block: &MapBlockNodes) {
        MeshgenTask::spawn(
            self.device.clone(),
            self.meshgen_tx.clone(),
            &self.map,
            blockpos,
            block,
        );
    }

    fn generate_mapblock_with_neighbors(&self, blockpos: MapBlockPos) {
        self.generate_mapblock(blockpos, self.map.get_block(&blockpos).unwrap());

        for dir in NEIGHBOR_DIRS {
            if let Some(n_blockpos) = blockpos.checked_add(dir)
                && let Some(n_block) = self.map.get_block(&n_blockpos)
            {
                self.generate_mapblock(n_blockpos, n_block);
            }
        }
    }

    fn process_network_command(&mut self, command: ToClientCommand) -> anyhow::Result<()> {
        match command {
            // TODO: check connection/auth state first
            ToClientCommand::Hello(spec) => {
                if spec.auth_mechs.first_srp {
                    // register
                    self.client
                        .send(ToServerCommand::FirstSrp(Box::new(FirstSrpSpec {
                            salt: vec![],
                            verification_key: vec![],
                            is_empty: false, // only used for "disallow empty passwords"
                        })))?;
                } else {
                    // cannot login as that would require actually implementing srp :)
                    panic!("received unsupported or invalid auth method");
                }
            }

            // TODO: check connection/auth state first
            ToClientCommand::AuthAccept(_spec) => {
                self.client
                    .send(ToServerCommand::Init2(Box::new(Init2Spec {
                        lang: Some(String::from("en")),
                    })))?;

                // TODO: wait for item definitions, wait for media announce, request media, wait for media, etc. first

                self.client
                    .send(ToServerCommand::ClientReady(Box::new(ClientReadySpec {
                        major_ver: 0,
                        minor_ver: 1,
                        patch_ver: 0,
                        reserved: 0,
                        full_ver: String::from("Cubetonic 0.1.0"),
                        formspec_ver: Some(8), // corresponds to proto ver 46
                    })))?;
            }

            ToClientCommand::Blockdata(spec) => {
                // TODO: Luanti only sends this after meshgen? batching?
                self.client
                    .send(ToServerCommand::GotBlocks(Box::new(GotBlocksSpec {
                        blocks: vec![spec.pos],
                    })))?;

                let blockpos = MapBlockPos::new(spec.pos).unwrap();
                let block = MapBlockNodes(spec.block.nodes.nodes);
                self.map.insert_block(blockpos, block);
                self.generate_mapblock_with_neighbors(blockpos);
            }

            ToClientCommand::Addnode(spec) => {
                if let Some(blockpos) = self.map.set_node(&MapNodePos(spec.pos), spec.node) {
                    self.generate_mapblock_with_neighbors(blockpos);
                }
            }

            ToClientCommand::Removenode(spec) => {
                if let Some(blockpos) = self.map.set_node(
                    &MapNodePos(spec.pos),
                    MapNode {
                        content_id: ContentId::AIR,
                        param1: 0,
                        param2: 0,
                    },
                ) {
                    self.generate_mapblock_with_neighbors(blockpos);
                }
            }

            _ => (),
        }

        Ok(())
    }

    fn process_main_event(&mut self, event: MainToClientEvent) -> anyhow::Result<()> {
        match event {
            MainToClientEvent::PlayerPos { pos, yaw, pitch } => {
                self.client
                    .send(ToServerCommand::Playerpos(Box::new(PlayerPosCommand {
                        player_pos: luanti_protocol::types::PlayerPos {
                            position: pos * BS,
                            speed: Vec3::ZERO,
                            pitch: pitch,
                            // stored inverted compared to Luanti, Luanti only
                            // inverts it when applying e.g. in camera.cpp
                            yaw: -yaw,
                            keys_pressed: 0,
                            // expected to be max of horizontal and vertical fov
                            // just give a high value so we get much data
                            fov: PI,
                            // just give a high value so we get much data
                            wanted_range: 255,
                            camera_inverted: false,
                            movement_speed: 0.0,
                            movement_direction: 0.0,
                        },
                    })))?;
            }
        }

        Ok(())
    }
}

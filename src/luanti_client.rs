use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use glam::I16Vec3;
use luanti_core::MapBlockNodes;
use luanti_protocol::LuantiClient;
use luanti_protocol::commands::client_to_server::{
    FirstSrpSpec, Init2Spec, InitSpec, ToServerCommand,
};
use luanti_protocol::commands::server_to_client::ToClientCommand;
use rand::Rng;

pub struct LuantiClientRunner {
    client: LuantiClient,
    data: LuantiClientDataShared,
}

pub type LuantiClientDataShared = Arc<Mutex<LuantiClientData>>;

#[derive(Default)]
pub struct LuantiClientData {
    pub connected: bool,
    // TODO: cannot use MapBlockPos ergonomically since inner field is private :\
    pub mapblocks: HashMap<I16Vec3, MapBlockNodes>,
}

impl LuantiClientRunner {
    pub fn spawn(client: LuantiClient, data: LuantiClientDataShared) {
        let mut runner = LuantiClientRunner { client, data };
        tokio::spawn(async move { runner.run().await });
    }

    async fn run(&mut self) {
        match self.run_inner().await {
            Ok(()) => (), // unreachable
            Err(err) => {
                println!("Disconnected: {}", err);

                let mut data = self.data.lock().unwrap();
                data.connected = false;
            }
        }
    }

    async fn run_inner(&mut self) -> anyhow::Result<()> {
        let mut user_name = String::from("test");
        user_name.push_str(&rand::rng().random_range(0..1000).to_string());

        self.client.send(ToServerCommand::Init(Box::new(InitSpec {
            serialization_ver_max: 29,
            supp_compr_modes: 0, // unused
            min_net_proto_version: 37,
            max_net_proto_version: 46, // appears to be the maximum supported by luanti-protocol
            user_name: user_name.clone(),
        })))?;

        loop {
            println!("Waiting for command...");
            let command = self.client.recv().await;
            println!("Received: {:?}", command);
            let command = command?;

            match command {
                // TODO: check connection/auth state first
                ToClientCommand::Hello(spec) => {
                    if spec.auth_mechs.first_srp {
                        // register
                        self.client
                            .send(ToServerCommand::FirstSrp(Box::new(FirstSrpSpec {
                                salt: vec![],
                                verification_key: vec![],
                                is_empty: false, // only used for "disallow empty names"
                            })))?;
                    } else {
                        // cannot login as that would require actually implementing srp :)
                        panic!("received unsupported or invalid auth method");
                    }
                }

                // TODO: check connection/auth state first
                ToClientCommand::AuthAccept(spec) => {
                    self.client
                        .send(ToServerCommand::Init2(Box::new(Init2Spec {
                            lang: Some(String::from("en")),
                        })))?;
                }

                ToClientCommand::Blockdata(spec) => {
                    let mut data = self.data.lock().unwrap();

                    if !data.mapblocks.contains_key(&spec.pos) {
                        data.mapblocks
                            .insert(spec.pos, MapBlockNodes(spec.block.nodes.nodes));
                    } else {
                        data.mapblocks.get_mut(&spec.pos).unwrap().0 = spec.block.nodes.nodes;
                    }
                }
                _ => (),
            }
        }
    }
}

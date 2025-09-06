use std::path::PathBuf;

use anyhow::{Context, anyhow};
use log::info;
use mlua::Lua;

pub struct LuaController {
    base_dir: PathBuf,
    l: Lua,
}

impl LuaController {
    fn get_base_dir() -> anyhow::Result<PathBuf> {
        let mut exe_dir = std::env::current_exe()?;
        exe_dir.pop();

        loop {
            let subdir = exe_dir.join("scriptsrc");
            match subdir.try_exists()? {
                true => {
                    info!("Found scriptsrc at {subdir:?}");
                    return Ok(subdir);
                }
                false => {
                    // Try parent directory
                    if !exe_dir.pop() {
                        return Err(anyhow!("Could not find a scriptsrc dir"));
                    }
                }
            }
        }
    }

    pub fn new() -> anyhow::Result<Self> {
        let base_dir = Self::get_base_dir()?;
        let l = Lua::new();

        let chunk = l.load(base_dir.join("init.lua"));
        chunk.exec().with_context(|| "Failed to load main script")?;

        Ok(Self { base_dir, l })
    }
}

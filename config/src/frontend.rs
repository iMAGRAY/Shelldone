use luahelper::impl_lua_conversion_dynamic;
use shelldone_dynamic::{FromDynamic, ToDynamic};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromDynamic, ToDynamic, Default)]
pub enum FrontEndSelection {
    #[default]
    OpenGL,
    WebGpu,
    Software,
}

/// Corresponds to <https://docs.rs/wgpu/latest/wgpu/struct.AdapterInfo.html>
#[derive(Debug, Clone, FromDynamic, ToDynamic)]
pub struct GpuInfo {
    pub name: String,
    pub device_type: String,
    pub backend: String,
    pub driver: Option<String>,
    pub driver_info: Option<String>,
    pub vendor: Option<u32>,
    pub device: Option<u32>,
}
impl_lua_conversion_dynamic!(GpuInfo);

impl fmt::Display for GpuInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "name={}, device_type={}, backend={}",
            self.name, self.device_type, self.backend
        )?;
        if let Some(driver) = &self.driver {
            write!(f, ", driver={driver}")?;
        }
        if let Some(driver_info) = &self.driver_info {
            write!(f, ", driver_info={driver_info}")?;
        }
        if let Some(vendor) = &self.vendor {
            write!(f, ", vendor={vendor}")?;
        }
        if let Some(device) = &self.device {
            write!(f, ", device={device}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromDynamic, ToDynamic, Default)]
pub enum WebGpuPowerPreference {
    #[default]
    LowPower,
    HighPerformance,
}

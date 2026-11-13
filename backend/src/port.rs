use std::time::Duration;

use crate::obd::Error;

pub trait AdapterPort: Send {
    fn name(&self) -> Option<String>;
    fn baud_rate(&self) -> Option<u32>;
    fn set_timeout(&mut self, timeout: Duration);
    fn clear_input(&mut self) -> Result<(), Error>;
    fn write_all_bytes(&mut self, data: &[u8]) -> std::io::Result<()>;
    fn read_bytes(&mut self, buf: &mut [u8]) -> std::io::Result<usize>;
}

pub struct DemoPort;

impl AdapterPort for DemoPort {
    fn name(&self) -> Option<String> {
        Some("DEMO MODE".to_string())
    }

    fn baud_rate(&self) -> Option<u32> {
        Some(0)
    }

    fn set_timeout(&mut self, _timeout: Duration) {}

    fn clear_input(&mut self) -> Result<(), Error> {
        Ok(())
    }

    fn write_all_bytes(&mut self, _data: &[u8]) -> std::io::Result<()> {
        Ok(())
    }

    fn read_bytes(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Ok(0)
    }
}

#[cfg(not(target_os = "android"))]
pub struct HardwarePort(pub Box<dyn serialport::SerialPort>);

#[cfg(not(target_os = "android"))]
impl AdapterPort for HardwarePort {
    fn name(&self) -> Option<String> {
        self.0.name()
    }

    fn baud_rate(&self) -> Option<u32> {
        self.0.baud_rate().ok()
    }

    fn set_timeout(&mut self, timeout: Duration) {
        let _ = self.0.set_timeout(timeout);
    }

    fn clear_input(&mut self) -> Result<(), Error> {
        self.0.clear(serialport::ClearBuffer::All).map_err(|error| {
            use serialport::ErrorKind::{Io, NoDevice};
            match error.kind() {
                NoDevice => Error::NoConnection,
                Io(_) => Error::AdapterReadWriteError(error.description),
                _ => Error::AdapterReadWriteError("unknown i/o error".to_string()),
            }
        })
    }

    fn write_all_bytes(&mut self, data: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        self.0.write_all(data)
    }

    fn read_bytes(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use std::io::Read;
        self.0.read(buf)
    }
}

#[cfg(target_os = "android")]
pub struct BluetoothPort {
    timeout: Duration,
    name: String,
}

#[cfg(target_os = "android")]
impl BluetoothPort {
    fn new(name: String) -> Self {
        Self {
            timeout: Duration::from_secs(1),
            name,
        }
    }
}

#[cfg(target_os = "android")]
impl Drop for BluetoothPort {
    fn drop(&mut self) {
        crate::bluetooth::close();
    }
}

#[cfg(target_os = "android")]
impl AdapterPort for BluetoothPort {
    fn name(&self) -> Option<String> {
        Some(self.name.clone())
    }

    fn baud_rate(&self) -> Option<u32> {
        Some(38400)
    }

    fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    fn clear_input(&mut self) -> Result<(), Error> {
        crate::bluetooth::clear_input();
        Ok(())
    }

    fn write_all_bytes(&mut self, data: &[u8]) -> std::io::Result<()> {
        crate::bluetooth::write(data)
    }

    fn read_bytes(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let bytes = crate::bluetooth::read(buf.len(), self.timeout)?;
        let count = bytes.len().min(buf.len());
        buf[..count].copy_from_slice(&bytes[..count]);
        Ok(count)
    }
}

#[cfg(target_os = "android")]
pub fn open_bluetooth(port: &str) -> Result<Box<dyn AdapterPort>, Error> {
    crate::bluetooth::connect(port).map_err(Error::BluetoothConnectionFailed)?;
    Ok(Box::new(BluetoothPort::new(port.to_string())))
}

#[cfg(not(target_os = "android"))]
pub fn open_hardware(port: &str, baud_rate: u32) -> Option<Box<dyn AdapterPort>> {
    serialport::new(port, baud_rate)
        .timeout(Duration::from_secs(1))
        .open()
        .ok()
        .map(|port| Box::new(HardwarePort(port)) as Box<dyn AdapterPort>)
}

pub fn list_serial_ports() -> Vec<(String, u32)> {
    #[cfg(target_os = "android")]
    {
        crate::bluetooth::bonded_names()
            .into_iter()
            .map(|name| (name, 38400))
            .collect()
    }

    #[cfg(not(target_os = "android"))]
    {
        use std::io::{Read, Write};

        let ports = match serialport::available_ports() {
            Ok(ports) => ports,
            Err(err) => {
                println!("error getting available serial ports: {err}");
                return Vec::new();
            }
        };

        const BAUD_RATES: [u32; 6] = [38400, 9600, 115200, 57600, 230400, 4800];
        let handles: Vec<_> = ports
            .into_iter()
            .map(|port| {
                std::thread::spawn(move || {
                    let port_name = port.port_name;
                    for baud_rate in BAUD_RATES.iter() {
                        let Ok(mut con) = serialport::new(&port_name, *baud_rate)
                            .timeout(Duration::from_millis(500))
                            .open()
                        else {
                            continue;
                        };

                        let _ = con.clear(serialport::ClearBuffer::All);

                        if con.write_all(b"ATZ\r").is_err() {
                            continue;
                        }

                        std::thread::sleep(Duration::from_millis(500));
                        let _ = con.clear(serialport::ClearBuffer::All);

                        if con.write_all(b"ATI\r").is_err() {
                            continue;
                        }

                        let mut buffer: [u8; 256] = [0u8; 256];
                        let mut total_read = 0;
                        loop {
                            match con.read(&mut buffer[total_read..]) {
                                Ok(n) if n > 0 => total_read += n,
                                Ok(_) => break,
                                Err(e)
                                    if e.kind() == std::io::ErrorKind::TimedOut
                                        || e.kind() == std::io::ErrorKind::WouldBlock =>
                                {
                                    break
                                }
                                Err(_) => break,
                            }
                        }

                        let response = String::from_utf8_lossy(&buffer[..total_read]);
                        let cleaned = response.trim_matches(|c: char| !c.is_ascii_graphic());

                        println!(
                            "- response: `{}` for port: {}",
                            cleaned.escape_debug(),
                            port_name
                        );

                        if cleaned.ends_with('>') || cleaned.contains("ELM") {
                            return Some((port_name, *baud_rate));
                        }
                    }
                    None
                })
            })
            .collect();

        let open_serial_ports: Vec<(String, u32)> = handles
            .into_iter()
            .filter_map(|handle| handle.join().ok().flatten())
            .collect();

        println!("Ports: {:?}", open_serial_ports);
        open_serial_ports
    }
}

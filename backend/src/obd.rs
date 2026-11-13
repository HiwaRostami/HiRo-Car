use sqlite::State;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::str::{self, FromStr};
use std::thread::sleep;
use std::time::{Duration, Instant};
use thiserror::Error;

use crate::cmd::{Command, CommandType};
use crate::paths::mode22_pids_db_path;
use crate::port::{AdapterPort, DemoPort};
use crate::response::Response;
use crate::scalar::{Scalar, Unit, UnitPreferences};
use crate::vin::VIN;

#[derive(Debug)]
pub enum BankNumber {
    Bank1,
    Bank2,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SensorNumber {
    Sensor1,
    Sensor2,
    Sensor3,
    Sensor4,
    Sensor5,
    Sensor6,
    Sensor7,
    Sensor8,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to establish connection with adapter")]
    ConnectionFailed,
    #[error("Bluetooth connection to the adapter failed: {0}")]
    BluetoothConnectionFailed(String),
    #[error("Attempted to perform a read/write action via serial connection, but no serial connection to adapter is active")]
    NoConnection,

    #[error("Failed to initialize connection via adapter sending command: {0}")]
    InitFailedSendingCommand(String),
    #[error("Could not initialize connection via adapter - Failed reading response")]
    InitFailedReadingResponse,
    #[error("No response containing 'OK' received from adapter while initializing connection")]
    InitFailedNoOKReceived,
    #[error("The Bluetooth adapter is connected, but the ECU did not answer the OBD-II probe: {0}")]
    EcuNotResponding(String),

    #[error("Invalid OBD protocol given while trying to connect. Expected number between 0-9, got: {0} ")]
    InvalidProtocol(u8),
    #[error("Received invalid or mangled response to command via serial connection")]
    InvalidResponse,
    #[error("Received response explicitly containing 'NODATA' thus no meaningful data was received")]
    NoData,
    #[error("No positive response received after sending OBD request to clear DTCs. Expected positive response '44' but got '{0}'")]
    DTCClearFailed(String),

    #[error("Error occured during I/O operation via serial communication. Error details (if any): {0}")]
    AdapterReadWriteError(String),

    #[error("ANDROID_UNSUPPORTED")]
    AndroidUnsupported,
}

pub enum Service {
    Mode01,
    Mode22,
}

#[derive(Default)]
pub struct OBD {
    connection: Option<Box<dyn AdapterPort>>,
    elm_version: Option<String>,
    freeze_frame_query: bool,
    protocol: u8,

    pub(crate) requests_path: String,
    pub(crate) record_requests: bool,
    pub(crate) replay_requests: bool,

    pub(crate) unit_preferences: UnitPreferences,
}

fn vin_text(bytes: &[u8]) -> Option<String> {
    let vin = std::str::from_utf8(bytes).ok()?.trim_matches('\0');
    (vin.len() == 17 && vin.bytes().all(|byte| byte.is_ascii_digit() || byte.is_ascii_uppercase()))
        .then(|| vin.to_string())
}

impl OBD {
    pub fn new() -> Self {
        Self {
            requests_path: crate::paths::requests_json_path(),
            ..Default::default()
        }
    }

    pub fn connect(&mut self, port: &str, baud_rate: u32, protocol: u8) -> Result<(), Error> {
        if port == "DEMO MODE" {
            self.replay_requests = true;
            self.connection = Some(Box::new(DemoPort));
            return Ok(());
        }

        self.replay_requests = false;
        self.record_requests = false;

        if self.connection.is_some() {
            return Ok(());
        }

        #[cfg(target_os = "android")]
        {
            let _ = baud_rate;
            self.connection = Some(crate::port::open_bluetooth(port)?);
        }

        #[cfg(not(target_os = "android"))]
        {
            self.connection = crate::port::open_hardware(port, baud_rate);
        }

        if !self.is_connected() {
            return Err(Error::ConnectionFailed);
        }

        let result = (|| {
            self.init()?;
            let mut command = match protocol {
                0 => Command::new_at(b"ATSP0"),
                1 => Command::new_at(b"ATSP1"),
                2 => Command::new_at(b"ATSP2"),
                3 => Command::new_at(b"ATSP3"),
                4 => Command::new_at(b"ATSP4"),
                5 => Command::new_at(b"ATSP5"),
                6 => Command::new_at(b"ATSP6"),
                7 => Command::new_at(b"ATSP7"),
                8 => Command::new_at(b"ATSP8"),
                9 => Command::new_at(b"ATSP9"),
                _ => return Err(Error::InvalidProtocol(protocol)),
            };

            self.send_command(&mut command)?;
            let reply = self.get_at_response()?;
            if !reply.formatted_response.as_deref().is_some_and(|reply| reply.contains("OK")) {
                return Err(Error::InitFailedNoOKReceived);
            }
            self.protocol = protocol;

            #[cfg(target_os = "android")]
            {
                self.send_command(&mut Command::new_pid(b"0100"))?;
                let response = self.read_until_with_timeout(b'>', Duration::from_secs(12))
                    .map_err(|error| Error::EcuNotResponding(error.to_string()))?;
                if !crate::frames::decode(&response).iter().any(|frame| {
                    frame.bytes.starts_with(&[0x41, 0x00]) && frame.bytes.len() >= 6
                }) {
                    return Err(Error::EcuNotResponding(response.trim().to_string()));
                }
            }

            Ok(())
        })();
        if result.is_err() {
            self.disconnect();
        }
        result
    }

    pub fn disconnect(&mut self) {
        if let Some(connection) = self.connection.take() {
            drop(connection);
            self.connection = None;
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }

    pub fn serial_port_name(&self) -> Option<String> {
        match &self.connection {
            Some(connection) => connection.name(),
            None => None,
        }
    }

    pub fn serial_port_baud_rate(&self) -> Option<u32> {
        match &self.connection {
            Some(connection) => connection.baud_rate(),
            None => None,
        }
    }

    pub fn get_open_serial_ports() -> Vec<(String, u32)> {
        crate::port::list_serial_ports()
    }

    pub fn init(&mut self) -> Result<(), Error> {
        // Initialization commands to send before
        // full communication can be established.
        // Without these, requests will always time out,
        // and the ECU wont understand what we're asking for.
        // Furthermore, causes unexpected behaviour when parsing the response.
        let commands = vec![
            Command::new_at(b"ATZ"),  // Reset all
            Command::new_at(b"ATE0"), // Echo off
            Command::new_at(b"ATL0"), // Linefeeds off
            Command::new_at(b"ATH1"), // Headers on
        ];

        for mut command in commands {
            // Cannot proceed with the initialization.
            // Refer to above. Furthermore, if we don't send a command
            // and read the buffer and then get junk values,
            // the program will be messed up.
            // Ensure the intiialization is 100% valid.

            let response = if self.replay_requests {
                self.get_recorded_response(&command)
            } else {
                self.send_command(&mut command)
                    .map_err(|_| Error::InitFailedSendingCommand(command.as_string()))?;
                self.get_at_response()?
            };

            if self.record_requests {
                self.save_request(&command, &response);
            }

            match (command.get_at(), response.formatted_response.as_deref()) {
                (b"ATZ", Some(data)) => {
                    self.elm_version = Some(data.to_owned());
                }
                (_, Some(data)) if data.contains("OK") => {}
                x => {
                    println!("{:?}", x);
                    return Err(Error::InitFailedNoOKReceived);
                }
            }
        }

        Ok(())
    }

    /// Toggles whether PID requests are redirected to freeze frame (service 02).
    ///
    /// If `self.freeze_frame_query` is `true`, all service 01 PID requests
    /// will be redirected to service 02 (freeze frame).
    ///
    /// Returns the updated value of `freeze_frame_query`.
    pub fn query_freeze_frame(&mut self, state: bool) {
        self.freeze_frame_query = state
    }

    pub fn read_from_user_input(&mut self) {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        loop {
            let mut input = String::new();

            let _ = stdout.write(b"\n> ");
            let _ = stdout.flush();
            let _ = stdin.read_line(&mut input);

            let message = input.replace("\n", "").replace("\r", "");
            if message == "exit" {
                break;
            }

            if self.send_command(&mut Command::new_arb(&message)).is_err() {
                println!("< error sending message {message}");
                continue;
            }

            if let Ok(response) = self.read_until(b'>') {
                if response.is_empty() {
                    continue;
                }

                let printable_raw = response.escape_default();
                println!("< '{printable_raw}'");
            } else {
                println!("< error receiving response for {message}");
            }
        }
    }

    pub fn send_command(&mut self, req: &mut Command) -> Result<(), Error> {
        // We don't need to send a command since we already
        // know what the respond will be.
        if self.replay_requests {
            return Ok(());
        }

        let mut cmd = req.as_bytes();
        if cmd.is_empty() {
            return Ok(());
        }
        cmd.push(b'\r');

        let write_result = {
            let stream = match self.connection.as_mut() {
                Some(stream) => stream,
                None => return Err(Error::NoConnection),
            };
            let _ = stream.set_timeout(Duration::from_secs(1));
            let _ = stream.clear_input();
            stream.write_all_bytes(&cmd)
        };

        if let Err(error) = write_result {
            self.connection = None;
            return Err(Error::AdapterReadWriteError(error.to_string()));
        }

        Ok(())
    }

    pub fn get_at_response(&mut self) -> Result<Response, Error> {
        let response = self.read_until(b'>')?;

        let meta_data = Response {
            raw_response: Some(response.clone()),
            formatted_response: Some(response.replace("\r", "")),

            ..Default::default()
        };

        Ok(meta_data)
    }

    pub fn get_pid_response(&mut self) -> Result<Response, Error> {
        let response = self.read_until(b'>')?;
        self.parse_pid_response(&response)
    }

    pub(crate) fn parse_pid_response(&self, raw_response: &str) -> Result<Response, Error> {
        let frames = crate::frames::decode(raw_response);
        let frame = frames.iter().find(|frame| matches!(frame.bytes.first(), Some(0x40..=0x7E)))
            .ok_or(if raw_response.contains("NO DATA") { Error::NoData } else { Error::InvalidResponse })?;
        let payload = frame.bytes.iter().map(|byte| format!("{byte:02X}")).collect::<Vec<_>>().join(" ");
        let service = payload.as_bytes();
        Ok(Response {
            service: [service[0], service[1]],
            payload_size: frame.bytes.len(),
            responding_ecus: frames.into_iter().map(|frame| frame.ecu).collect(),
            formatted_response: Some(payload.clone()),
            raw_response: Some(raw_response.to_string()),
            payload: Some(payload),
        })
    }

    pub fn get_service_supported_pids(&mut self, service: &str) -> HashMap<String, Vec<String>> {
        // Service must be 2 characters long
        // An example of a service would be
        // '01' '05' or '09'
        if service.len() != 2 {
            println!("get_service_supported_pids; service ({service}) must be 2 characters long.");
            return HashMap::new();
        }

        let service_as_bytes = service.as_bytes();

        // A hashmap of supported pids for different ECUs
        // Key -> ECU Name
        // Value -> List of supported pids as strings
        let mut supported_pids: HashMap<String, Vec<String>> = HashMap::new();

        // The pids to request service 'service' to get a list of
        // supported pids by the car for that service
        let mut requests = Vec::new();

        if service == "01" {
            requests = vec!["00", "20", "40", "60", "80", "A0", "C0"];
        } else if service == "05" || service == "09" {
            requests = vec!["00"];
        }

        for request_pid in requests {
            let request_pid_bytes = request_pid.as_bytes();
            let command = [
                service_as_bytes[0],
                service_as_bytes[1],
                request_pid_bytes[0],
                request_pid_bytes[1],
            ];
            let response = self.query(Command::new_pid(&command));

            let Ok(pid) = u8::from_str_radix(request_pid, 16) else { continue };
            let Ok(service_id) = u8::from_str_radix(service, 16) else { continue };
            for frame in crate::frames::decode(&response.raw_response().unwrap_or_default()) {
                let [reply, response_pid, mask @ ..] = frame.bytes.as_slice() else { continue };
                if *reply != service_id + 0x40 || *response_pid != pid || mask.len() < 4 {
                    continue;
                }
                let supported = supported_pids.entry(frame.ecu).or_default();
                for (index, byte) in mask[..4].iter().enumerate() {
                    for bit in 0..8 {
                        if byte & (0x80 >> bit) != 0 {
                            supported.push(format!("{:02X}", usize::from(pid) + 8 * index + bit + 1));
                        }
                    }
                }
            }
        }

        supported_pids
    }

    pub fn extract_ecu_names(response: &str) -> Vec<String> {
        response
            .lines()
            .filter_map(|line| {
                let split: Vec<&str> = line.split_whitespace().collect();
                if split.len() < 3 {
                    return None;
                }

                let ecu_name = split[0].to_string();
                if ecu_name.len() != 3 {
                    // can ids are 3 character hex strings
                    return None;
                }

                Some(ecu_name)
            })
            .collect()
    }

    pub fn extract_payload_size(response: &str) -> u32 {
        response
            .split_whitespace()
            .nth(1)
            .and_then(|s| u32::from_str_radix(s, 16).ok())
            .unwrap_or(0)
    }

    pub(crate) fn strip_ecu_names(response: &mut String, ecu_names: &[String]) {
        for ecu_name in ecu_names.iter() {
            *response = response.replace(ecu_name, "")
        }
    }

    pub(crate) fn read_until(&mut self, until: u8) -> Result<String, Error> {
        self.read_until_with_timeout(until, Duration::from_secs(2))
    }

    fn read_until_with_timeout(&mut self, until: u8, timeout: Duration) -> Result<String, Error> {
        if self.replay_requests {
            return Ok(String::new());
        }

        let port = match &mut self.connection {
            Some(port) => port,
            None => return Err(Error::NoConnection),
        };

        let mut buffer = [0u8; 1];
        let mut response = String::new();
        let deadline = Instant::now() + timeout;

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Error::AdapterReadWriteError("Timed out waiting for adapter response".into()));
            }
            port.set_timeout(remaining);
            match port.read_bytes(&mut buffer) {
                Ok(1) => {
                    let byte = buffer[0];
                    if byte == until {
                        break;
                    }

                    response.push(byte as char);
                }
                Ok(0) => std::thread::sleep(std::time::Duration::from_millis(10)),
                Ok(_) => break,
                Err(error) => return Err(Error::AdapterReadWriteError(error.to_string())),
            }
        }

        if response.contains("UNABLE TO CONNECT") {
            println!("unable to connect");
            Err(Error::NoConnection)
        } else {
            Ok(response)
        }
    }

    pub fn read_vin_raw(&mut self) -> Option<String> {
        self.send_command(&mut Command::new_pid(b"0902")).ok()?;
        let raw = self.read_until(b'>').ok()?;
        let mut legacy: HashMap<String, Vec<(u8, Vec<u8>)>> = HashMap::new();
        for frame in crate::frames::decode(&raw) {
            let [0x49, 0x02, record, data @ ..] = frame.bytes.as_slice() else { continue };
            if frame.can || data.len() >= 17 {
                if let Some(vin) = vin_text(data) {
                    return Some(vin);
                }
            } else {
                legacy.entry(frame.ecu).or_default().push((*record, data.to_vec()));
            }
        }
        legacy.into_values().find_map(|mut parts| {
            parts.sort_by_key(|(index, _)| *index);
            let bytes: Vec<_> = parts.into_iter().flat_map(|(_, bytes)| bytes).collect();
            vin_text(&bytes)
        })
    }

    pub fn ecu_profile(&mut self) -> serde_json::Value {
        let vin = self.read_vin_raw();
        let protocol = self
            .query(Command::new_at(b"ATDP"))
            .formatted_response
            .unwrap_or_default();
        let trouble_codes: Vec<_> = self
            .get_trouble_codes()
            .into_iter()
            .map(|code| {
                serde_json::json!({
                    "code": code.dtc,
                    "description": code.description,
                    "permanent": code.permanant,
                    "category": code.category.to_string(),
                })
            })
            .collect();
        let supported = self.get_service_supported_pids("01");

        serde_json::json!({
            "savedAt": chrono::Utc::now().to_rfc3339(),
            "adapter": self.serial_port_name(),
            "elm": self.elm_version,
            "protocol": protocol,
            "vin": vin,
            "troubleCodes": trouble_codes,
            "supportedPids": supported,
        })
    }

    pub fn get_vin(&mut self) -> Option<VIN> {
        let vin = self.read_vin_raw()?;
        match VIN::new(&vin) {
            Ok(vin) => Some(vin),
            Err(err) => {
                println!("error when getting vin {vin}. {err}");
                None
            }
        }
    }

    pub(crate) fn format_response(response: &str) -> String {
        let chunks = response
            .as_bytes()
            .chunks(2)
            .map(|pair| std::str::from_utf8(pair).unwrap_or(""))
            .collect::<Vec<&str>>();

        chunks.join(" ")
    }

    pub fn query(&mut self, mut request: Command) -> Response {
        if self.freeze_frame_query && *request.command_type() == CommandType::PIDCommand {
            let pid = request.get_pid();
            if pid.starts_with(b"01") {
                request.set_pid(&[b'0', b'2', pid[2], pid[3]]);
            }
        }

        match self.send_command(&mut request) {
            Ok(_) => (),
            Err(err) => {
                println!(
                    "{}\tAT: '{}' - PID: '{}' ",
                    err,
                    String::from_utf8_lossy(request.get_at()),
                    String::from_utf8(request.get_pid().to_vec()).unwrap_or_default()
                );
                return Response::default();
            }
        };

        let response = if self.replay_requests {
            self.get_recorded_response(&request)
        } else {
            if *request.command_type() == CommandType::ATCommand {
                self.get_at_response().unwrap_or_else(|_| Response::no_data())
            } else {
                self.get_pid_response().unwrap_or_else(|_| Response::no_data())
            }
        };

        if self.record_requests {
            self.save_request(&request, &response);
        }

        response
    }

    pub fn set_unit_preferences(&mut self, preferences: UnitPreferences) {
        self.unit_preferences = preferences;
    }

    pub fn get_protocol_name(&mut self) -> Result<String, Error> {
        let mut request = Command::new_at(b"AT DP");
        self.send_command(&mut request)?;

        let response = if self.replay_requests {
            self.get_recorded_response(&request)
        } else {
            self.get_at_response().unwrap_or(Response::no_data())
        };

        if self.record_requests {
            self.save_request(&request, &response);
        }

        response.formatted_response.ok_or(Error::InvalidResponse)
    }

    pub fn get_protocol_number(&self) -> u8 {
        self.protocol
    }

    /// Test and run Mode 22 pids from
    /// /data/model-pids.sqlite
    pub fn test_mode_22_pids(&mut self, vin: &VIN) {
        // This does not work when replaying requests
        if self.replay_requests {
            return;
        }

        // Get a Mode 22 pids for the model from the vin
        // Run them all, see if the output is valid.
        // Calculate equation and display
        // Sleep for a short period of time.
        // Repeat
        let model = match vin.get_engine_manufacturer() {
            Ok(em) => em,
            Err(_) => return,
        };

        // connect to mode 22 database
        let con = match sqlite::Connection::open(mode22_pids_db_path()) {
            Ok(con) => con,
            Err(err) => {
                println!("when connecting to mode22 database: {err}");
                return;
            }
        };

        let query = "SELECT * FROM vehicle_pids WHERE model = ?";
        let mut statement = match con.prepare(query) {
            Ok(statement) => statement,
            Err(err) => {
                println!("when sanitizing statement {query}: {err}");
                return;
            }
        };

        match statement.bind((1, model.as_str())) {
            Ok(_) => {}
            Err(err) => {
                println!("when binding model '{}' to query {query}: {err}", model);
                return;
            }
        };

        while let Ok(State::Row) = statement.next() {
            let pid = statement
                .read::<String, _>("pid")
                .expect("reading column pid");
            let equation = statement
                .read::<String, _>("equation")
                .expect("reading column equation");
            let unit = statement
                .read::<String, _>("unit")
                .expect("reading column unit");
            let description = statement
                .read::<String, _>("description")
                .expect("reading description");

            let command = Command::new_arb(&pid);
            self.query(command).map_no_data(|response| {
                match self.calculate_dynamic_equation(&equation, &unit, &response) {
                    Ok(value) => {
                        println!("successfully calculated pid {pid}. equation: {equation}. unit {unit}");
                        println!("description: {description}");
                        println!("calculated: {value}");
                        value
                    },
                    Err(err) => {
                        println!("error trying to calculate pid {pid}. equation: {equation}. unit {unit}");
                        println!("description: {description}");
                        println!("error: {err}");
                        Scalar::no_data()
                    }
                }
            });

            sleep(Duration::from_millis(500));
        }
    }

    pub fn calculate_dynamic_equation(
        &mut self,
        equation: &str,
        unit: &str,
        response: &Response,
    ) -> Result<Scalar, Box<dyn std::error::Error>> {
        use evalexpr::*;

        if *response.get_payload_size() == 0 {
            return Ok(Scalar::no_data());
        }

        let equation = equation.to_uppercase();

        // TODO: fix this ugly code.
        let mut context: HashMapContext<DefaultNumericTypes> = HashMapContext::new();
        let variables = [
            ("A", response.a_value()),
            ("B", response.b_value()),
            ("C", response.c_value()),
            ("D", response.d_value()),
            ("E", response.e_value()),
        ];

        for (variable, value) in variables {
            if equation.contains(variable) {
                context.set_value(variable.into(), Value::from_float(value as f64))?;
            }
        }

        if equation.contains("A") {
            context.set_value("A".into(), Value::from_float(response.a_value() as f64))?;
        }
        if equation.contains("B") {
            context.set_value("B".into(), Value::from_float(response.b_value() as f64))?;
        }
        if equation.contains("C") {
            context.set_value("C".into(), Value::from_float(response.c_value() as f64))?;
        }
        if equation.contains("D") {
            context.set_value("D".into(), Value::from_float(response.d_value() as f64))?;
        }
        if equation.contains("E") {
            context.set_value("E".into(), Value::from_float(response.e_value() as f64))?;
        }

        match eval_float_with_context(&equation, &context) {
            Ok(res) => Ok(Scalar::new(
                res as f32,
                Unit::from_str(unit).unwrap_or(Unit::Unknown),
                Some(self.unit_preferences),
            )),
            Err(err) => {
                println!("when evaluating dynamic equation {equation}. {err}");
                Err(Box::new(err))
            }
        }
    }
}

use std::fs;

use crate::{
    cmd::{Command, CommandType},
    obd::OBD,
    response::Response,
};
use rand::seq::IndexedRandom;
use serde_json::{json, Value};

impl OBD {
    pub fn record_requests(&mut self, state: bool, path: String) {
        if state {
            self.requests_path = path;

            match fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.requests_path)
            {
                Ok(_) => {
                    self.record_requests = true;
                }
                Err(e) => {
                    println!("File creation to save requests failed: {e}. Not recording requests.");
                    self.record_requests = false;
                }
            }
        } else {
            self.record_requests = false;
        }

        println!("Recording requests: {state:?}");
    }

    pub fn replay_requests(&mut self, state: bool) {
        // stop recording when replaying
        if state {
            self.record_requests = false;
        }

        self.replay_requests = state;
        println!("Replaying requests: {state:?}");
    }

    pub(crate) fn save_request(&mut self, request: &Command, response: &Response) {
        if let Err(error) = self.append_recording(request, response) {
            self.record_requests = false;
            eprintln!("Stopped recording OBD responses: {error}");
        }
    }

    fn append_recording(&self, request: &Command, response: &Response) -> std::io::Result<()> {
        let contents = fs::read_to_string(&self.requests_path)?;
        let mut entries: Vec<Value> = if contents.trim().is_empty() {
            Vec::new()
        } else {
            serde_json::from_str(&contents).map_err(std::io::Error::other)?
        };
        entries.push(json!({
            "request": request.as_string(),
            "request_type": request.command_type(),
            "response": response.raw_response.as_deref().unwrap_or("")
        }));
        let json = serde_json::to_vec_pretty(&entries).map_err(std::io::Error::other)?;
        let path = std::path::Path::new(&self.requests_path);
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, json)?;
        fs::rename(temporary, path)
    }

    pub(crate) fn get_recorded_response(&self, request: &Command) -> Response {
        let contents_json: Vec<Value> = {
            let contents = match fs::read_to_string(&self.requests_path) {
                Ok(contents) => contents,
                Err(_) => return Response::no_data(),
            };

            if contents.trim().is_empty() {
                vec![]
            } else {
                serde_json::from_str(&contents).unwrap_or_else(|_| vec![])
            }
        };

        if contents_json.is_empty() {
            return Response::no_data();
        }

        let mut related_requests = Vec::new();
        for value in contents_json.iter() {
            if value["request"] == request.as_string() {
                related_requests.push(value);
            }
        }

        if related_requests.is_empty() {
            return Response::no_data();
        }

        // Randomly select response to use from related_requests
        if let Some(value) = related_requests.choose(&mut rand::rng()) {
            let escaped_response = value["response"].as_str().unwrap_or_default();
            if value["request_type"] == json!(CommandType::PIDCommand) {
                return self
                    .parse_pid_response(escaped_response)
                    .unwrap_or(Response::no_data());
            }
            return Response::new(escaped_response.to_string(), escaped_response.to_string());
        }
        Response::no_data()
    }
}

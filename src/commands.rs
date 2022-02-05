use serde::Deserialize;
use std::fs::read_to_string;

#[derive(Deserialize, Debug)]
#[allow(non_snake_case)]
#[serde(rename_all = "snake_case")]
pub struct CommandsJson {
    pub increaseSimilarityThreshold: Vec<String>,
    pub decreaseSimilarityThreshold: Vec<String>,
    pub increaseMosaicSize: Vec<String>,
    pub decreaseMosaicSize: Vec<String>,
    pub help: Vec<String>,
}

pub fn get_commands_json() -> CommandsJson {
    let json = match read_to_string("commands.json") {
        Ok(result) => result,
        _ => {
            println!("No commands.json found. Defaulting to empty json - commands will NOT work!");

            String::from("
                {\"increaseSimilarityThreshold\": [],\"decreaseSimilarityThreshold\": [],\"increaseMosaicSize\": [],\"decreaseMosaicSize\": [],\"help\": []}",
            )
        }
    };

    let r: CommandsJson = serde_json::from_str(&json).unwrap();

    return r;
}

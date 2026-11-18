use crate::providers::{ChatModel, ChatRequest};
use crate::Error;

pub fn validate(model: &dyn ChatModel) -> Result<(), Error> {
    let request = ChatRequest::new(
        "You are a health check endpoint. Reply with OK only.",
        "Reply exactly OK.",
        0.0,
        32,
    );
    let response = model.complete(&request)?;
    if response.to_uppercase().contains("OK") {
        Ok(())
    } else {
        Err(Error::Unparseable(
            "ai api returned an unexpected test response",
        ))
    }
}

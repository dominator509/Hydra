use cdm::Entity;
use governor::{Clock, EnvelopeState, ExecuteToken};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ExecuteError {
    #[error(transparent)]
    Store(#[from] store::StoreError),
    #[error(transparent)]
    Domain(#[from] governor::DomainError),
    #[error("executor only runs approved envelopes")]
    EnvelopeNotApproved,
    #[error("unsupported envelope execution path {domain}/{action}/{kind:?}")]
    UnsupportedEnvelope {
        domain: String,
        action: String,
        kind: Option<String>,
    },
    #[error("missing string payload field '{0}'")]
    MissingPayloadField(&'static str),
}

pub struct Executor {
    store: store::Store,
}

impl Executor {
    pub fn new(store: store::Store) -> Self {
        Self { store }
    }

    pub async fn execute(
        &self,
        token: ExecuteToken,
        clock: &dyn Clock,
    ) -> Result<governor::ActionEnvelope, ExecuteError> {
        let mut envelope = self.store.envelopes.get_by_id(token.envelope_id()).await?;
        if envelope.state != EnvelopeState::Approved {
            return Err(ExecuteError::EnvelopeNotApproved);
        }

        envelope.transition(EnvelopeState::Executing, "executor", clock)?;
        envelope = self
            .store
            .envelopes
            .save(envelope.tenant, &envelope)
            .await?;

        self.apply(&envelope).await?;

        envelope.transition(EnvelopeState::Executed, "executor", clock)?;
        let envelope = self
            .store
            .envelopes
            .save(envelope.tenant, &envelope)
            .await?;
        Ok(envelope)
    }

    async fn apply(&self, envelope: &governor::ActionEnvelope) -> Result<(), ExecuteError> {
        match (
            envelope.domain.as_str(),
            envelope.action.as_str(),
            envelope.kind.as_deref(),
        ) {
            ("pipeline", "move_stage", Some("deal")) => {
                let stage_id = envelope
                    .payload
                    .get("stage")
                    .or_else(|| envelope.payload.get("stage_id"))
                    .and_then(serde_json::Value::as_str)
                    .ok_or(ExecuteError::MissingPayloadField("stage"))?;

                for target in &envelope.targets {
                    let entity = self.store.entities.get(envelope.tenant, *target).await?;
                    let mut body = entity.body.clone();
                    body["stage_id"] = json!(stage_id);
                    let next = Entity {
                        id: entity.id,
                        kind: entity.kind,
                        tenant: entity.tenant,
                        body,
                        origin: entity.origin,
                        origin_ref: entity.origin_ref,
                        version: entity.version + 1,
                    };
                    let _ = self.store.entities.upsert(envelope.tenant, next).await?;
                }

                Ok(())
            }
            _ => Err(ExecuteError::UnsupportedEnvelope {
                domain: envelope.domain.clone(),
                action: envelope.action.clone(),
                kind: envelope.kind.clone(),
            }),
        }
    }
}

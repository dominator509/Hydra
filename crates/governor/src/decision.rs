mod sealed {
    use uuid::Uuid;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ExecuteToken {
        tenant: Uuid,
        envelope_id: Uuid,
    }

    impl ExecuteToken {
        pub(crate) fn new(tenant: Uuid, envelope_id: Uuid) -> Self {
            Self {
                tenant,
                envelope_id,
            }
        }

        pub fn tenant(self) -> Uuid {
            self.tenant
        }

        pub fn envelope_id(self) -> Uuid {
            self.envelope_id
        }
    }
}

pub use sealed::ExecuteToken;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Block(String),
    SuggestOnly,
    Queue,
    Execute(ExecuteToken),
}

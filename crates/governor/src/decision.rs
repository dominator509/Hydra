mod sealed {
    use uuid::Uuid;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ExecuteToken {
        envelope_id: Uuid,
    }

    impl ExecuteToken {
        pub(crate) fn new(envelope_id: Uuid) -> Self {
            Self { envelope_id }
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

use floe_agent_contract::{AgentFailure, BoxFuture};

use crate::SessionArchiveRepository;

pub(crate) struct ConversationArchiveReader<'a, Repository> {
    repository: &'a Repository,
}

impl<'a, Repository> ConversationArchiveReader<'a, Repository> {
    pub(crate) fn new(repository: &'a Repository) -> Self {
        Self { repository }
    }
}

impl<Repository: SessionArchiveRepository> floe_context::ArchiveReader
    for ConversationArchiveReader<'_, Repository>
{
    fn read_archive<'a>(
        &'a self,
        request: &'a floe_context::ArchiveReadRequest,
    ) -> BoxFuture<'a, Result<floe_context::ArchiveSnapshot, AgentFailure>> {
        self.repository.read_archive(request)
    }
}

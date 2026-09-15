use floe_agent_contract::{
    AgentFailure, ArchiveReadRequest, ArchiveReader, ArchiveSnapshot, BoxFuture,
};

use crate::SessionArchiveRepository;

pub(crate) struct ConversationArchiveReader<'a, Repository> {
    repository: &'a Repository,
}

impl<'a, Repository> ConversationArchiveReader<'a, Repository> {
    pub(crate) fn new(repository: &'a Repository) -> Self {
        Self { repository }
    }
}

impl<Repository: SessionArchiveRepository> ArchiveReader
    for ConversationArchiveReader<'_, Repository>
{
    fn read_archive<'a>(
        &'a self,
        request: &'a ArchiveReadRequest,
    ) -> BoxFuture<'a, Result<ArchiveSnapshot, AgentFailure>> {
        self.repository.read_archive(request)
    }
}

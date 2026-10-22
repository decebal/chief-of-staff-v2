pub mod agents;
pub mod auth;
pub mod claude;
pub mod context;
pub mod dash;
pub mod gather;
pub mod handoff;
pub mod history;
pub mod mcp;
pub mod pipeline;
pub mod proc;
pub mod report;
pub mod reports;
pub mod state;
pub mod store;
pub mod track;
pub mod tunnel;

/// Read-only surface. With `--permission-mode dontAsk`, any tool not listed is
/// denied — sending mail, editing Jira, reading or writing files included.
pub const READ_ONLY: &[&str] = &[
    "ToolSearch",
    "mcp__claude_ai_Google_Calendar__list_events",
    "mcp__claude_ai_Google_Calendar__get_event",
    "mcp__claude_ai_Google_Calendar__search_events",
    "mcp__claude_ai_Gmail__search_threads",
    "mcp__claude_ai_Gmail__get_thread",
    "mcp__claude_ai_Gmail__get_message",
    "mcp__claude_ai_Atlassian_Rovo__atlassianUserInfo",
    "mcp__claude_ai_Atlassian_Rovo__getAccessibleAtlassianResources",
    "mcp__claude_ai_Atlassian_Rovo__searchJiraIssuesUsingJql",
    "mcp__claude_ai_Atlassian_Rovo__getJiraIssue",
    "mcp__claude_ai_Atlassian_Rovo__searchConfluenceUsingCql",
    "mcp__claude_ai_Atlassian_Rovo__getConfluencePage",
    "mcp__claude_ai_Atlassian_Rovo__search",
    "mcp__claude_ai_Notion__notion-search",
    "mcp__claude_ai_Notion__notion-fetch",
    "mcp__claude_ai_Notion__notion-query-meeting-notes",
];

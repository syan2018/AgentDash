export {
  connectAgentRunTerminalFeed,
  connectWorkspacePresentationFeed,
} from "./model/agentRunProductProjectionFeeds";
export {
  connectProductProjectionFeed,
  type ProductProjectionFeedConnection,
  type ProductProjectionFeedDependencies,
  type ProductProjectionFeedLifecycle,
  type ProductProjectionFeedObserver,
} from "./model/productProjectionFeed";
export {
  projectAgentRunTerminalChanges,
  projectAgentRunTerminalSnapshot,
} from "./model/terminalProjectionConsumer";

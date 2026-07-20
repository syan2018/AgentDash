mod child_evidence;
mod commands;
mod gate_wait_policy;
mod outcome;
mod resolver;

pub use commands::{
    CompleteChildResultGateCommand, LifecycleGateCommand, OpenCompanionGateCommand,
    OpenParentRequestGateCommand, ResolveGatePayloadCommand, ResolveParentRequestGateCommand,
    RespondHumanGateCommand,
};
pub use gate_wait_policy::{
    GateProducerTerminalConvergenceOutcome, GateProducerTerminalConvergenceOutcomeKind,
    GateProducerTerminalConvergenceResult, GateProducerTerminalConvergenceService,
    GateProducerTerminalEvent, GateWakeTargetRuntimeThreadQuery, ProducerLastMessageEvidence,
    RuntimeTerminalDiagnostic,
};
pub use outcome::{
    CompanionChildResultDeliveryIntent, CompanionHumanResponseDeliveryIntent,
    CompanionParentRequestDeliveryIntent, CompanionParentResponseDeliveryIntent,
    GateDeliveryIntent, GateInputHandoffWakeIntent, GateTransitionKind, GateTransitionOutcome,
};
pub use resolver::LifecycleGateResolver;

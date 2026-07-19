// @generated from the pinned Codex App Server schema.
pub(crate) fn deserialize_optional_explicit_null<'de, D, T>(
    deserializer: D,
) -> Result<Option<Option<T>>, D::Error>
where
    D: ::serde::Deserializer<'de>,
    T: ::serde::Deserialize<'de>,
{
    <Option<T> as ::serde::Deserialize>::deserialize(deserializer).map(Some)
}
pub mod thread_item {
    #[doc = r" Error types."]
    pub mod error {
        #[doc = r" Error from a `TryFrom` or `FromStr` implementation."]
        pub struct ConversionError(::std::borrow::Cow<'static, str>);
        impl ::std::error::Error for ConversionError {}
        impl ::std::fmt::Display for ConversionError {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                ::std::fmt::Display::fmt(&self.0, f)
            }
        }
        impl ::std::fmt::Debug for ConversionError {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                ::std::fmt::Debug::fmt(&self.0, f)
            }
        }
        impl From<&'static str> for ConversionError {
            fn from(value: &'static str) -> Self {
                Self(value.into())
            }
        }
        impl From<String> for ConversionError {
            fn from(value: String) -> Self {
                Self(value.into())
            }
        }
    }
    #[doc = "A path that is guaranteed to be absolute and normalized (though it is not guaranteed to be canonicalized or exist on the filesystem).\n\nIMPORTANT: When deserializing an `AbsolutePathBuf`, a base path must be set using [AbsolutePathBufGuard::new]. If no base path is set, the deserialization will fail unless the path being deserialized is already absolute."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"A path that is guaranteed to be absolute and normalized (though it is not guaranteed to be canonicalized or exist on the filesystem).\\n\\nIMPORTANT: When deserializing an `AbsolutePathBuf`, a base path must be set using [AbsolutePathBufGuard::new]. If no base path is set, the deserialization will fail unless the path being deserialized is already absolute.\","]
    #[doc = "  \"type\": \"string\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    #[serde(transparent)]
    pub struct AbsolutePathBuf(pub ::std::string::String);
    impl ::std::ops::Deref for AbsolutePathBuf {
        type Target = ::std::string::String;
        fn deref(&self) -> &::std::string::String {
            &self.0
        }
    }
    impl ::std::convert::From<AbsolutePathBuf> for ::std::string::String {
        fn from(value: AbsolutePathBuf) -> Self {
            value.0
        }
    }
    impl ::std::convert::From<::std::string::String> for AbsolutePathBuf {
        fn from(value: ::std::string::String) -> Self {
            Self(value)
        }
    }
    impl ::std::str::FromStr for AbsolutePathBuf {
        type Err = ::std::convert::Infallible;
        fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
            Ok(Self(value.to_string()))
        }
    }
    impl ::std::fmt::Display for AbsolutePathBuf {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            self.0.fmt(f)
        }
    }
    #[doc = "`ByteRange`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"end\","]
    #[doc = "    \"start\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"end\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"uint\","]
    #[doc = "      \"minimum\": 0.0"]
    #[doc = "    },"]
    #[doc = "    \"start\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"uint\","]
    #[doc = "      \"minimum\": 0.0"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ByteRange {
        pub end: u32,
        pub start: u32,
    }
    #[doc = "`CodexConversationRoot`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"CodexConversationRoot\","]
    #[doc = "  \"$ref\": \"#/definitions/ThreadItem\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(transparent)]
    pub struct CodexConversationRoot(pub ThreadItem);
    impl ::std::ops::Deref for CodexConversationRoot {
        type Target = ThreadItem;
        fn deref(&self) -> &ThreadItem {
            &self.0
        }
    }
    impl ::std::convert::From<CodexConversationRoot> for ThreadItem {
        fn from(value: CodexConversationRoot) -> Self {
            value.0
        }
    }
    impl ::std::convert::From<ThreadItem> for CodexConversationRoot {
        fn from(value: ThreadItem) -> Self {
            Self(value)
        }
    }
    #[doc = "`CollabAgentState`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"status\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"message\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"status\": {"]
    #[doc = "      \"$ref\": \"#/definitions/CollabAgentStatus\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct CollabAgentState {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub message: ::std::option::Option<::std::string::String>,
        pub status: CollabAgentStatus,
    }
    #[doc = "`CollabAgentStatus`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"pendingInit\","]
    #[doc = "    \"running\","]
    #[doc = "    \"interrupted\","]
    #[doc = "    \"completed\","]
    #[doc = "    \"errored\","]
    #[doc = "    \"shutdown\","]
    #[doc = "    \"notFound\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum CollabAgentStatus {
        #[serde(rename = "pendingInit")]
        PendingInit,
        #[serde(rename = "running")]
        Running,
        #[serde(rename = "interrupted")]
        Interrupted,
        #[serde(rename = "completed")]
        Completed,
        #[serde(rename = "errored")]
        Errored,
        #[serde(rename = "shutdown")]
        Shutdown,
        #[serde(rename = "notFound")]
        NotFound,
    }
    impl ::std::fmt::Display for CollabAgentStatus {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::PendingInit => f.write_str("pendingInit"),
                Self::Running => f.write_str("running"),
                Self::Interrupted => f.write_str("interrupted"),
                Self::Completed => f.write_str("completed"),
                Self::Errored => f.write_str("errored"),
                Self::Shutdown => f.write_str("shutdown"),
                Self::NotFound => f.write_str("notFound"),
            }
        }
    }
    impl ::std::str::FromStr for CollabAgentStatus {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "pendingInit" => Ok(Self::PendingInit),
                "running" => Ok(Self::Running),
                "interrupted" => Ok(Self::Interrupted),
                "completed" => Ok(Self::Completed),
                "errored" => Ok(Self::Errored),
                "shutdown" => Ok(Self::Shutdown),
                "notFound" => Ok(Self::NotFound),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for CollabAgentStatus {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for CollabAgentStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for CollabAgentStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`CollabAgentTool`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"spawnAgent\","]
    #[doc = "    \"sendInput\","]
    #[doc = "    \"resumeAgent\","]
    #[doc = "    \"wait\","]
    #[doc = "    \"closeAgent\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum CollabAgentTool {
        #[serde(rename = "spawnAgent")]
        SpawnAgent,
        #[serde(rename = "sendInput")]
        SendInput,
        #[serde(rename = "resumeAgent")]
        ResumeAgent,
        #[serde(rename = "wait")]
        Wait,
        #[serde(rename = "closeAgent")]
        CloseAgent,
    }
    impl ::std::fmt::Display for CollabAgentTool {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::SpawnAgent => f.write_str("spawnAgent"),
                Self::SendInput => f.write_str("sendInput"),
                Self::ResumeAgent => f.write_str("resumeAgent"),
                Self::Wait => f.write_str("wait"),
                Self::CloseAgent => f.write_str("closeAgent"),
            }
        }
    }
    impl ::std::str::FromStr for CollabAgentTool {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "spawnAgent" => Ok(Self::SpawnAgent),
                "sendInput" => Ok(Self::SendInput),
                "resumeAgent" => Ok(Self::ResumeAgent),
                "wait" => Ok(Self::Wait),
                "closeAgent" => Ok(Self::CloseAgent),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for CollabAgentTool {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for CollabAgentTool {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for CollabAgentTool {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`CollabAgentToolCallStatus`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"inProgress\","]
    #[doc = "    \"completed\","]
    #[doc = "    \"failed\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum CollabAgentToolCallStatus {
        #[serde(rename = "inProgress")]
        InProgress,
        #[serde(rename = "completed")]
        Completed,
        #[serde(rename = "failed")]
        Failed,
    }
    impl ::std::fmt::Display for CollabAgentToolCallStatus {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::InProgress => f.write_str("inProgress"),
                Self::Completed => f.write_str("completed"),
                Self::Failed => f.write_str("failed"),
            }
        }
    }
    impl ::std::str::FromStr for CollabAgentToolCallStatus {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "inProgress" => Ok(Self::InProgress),
                "completed" => Ok(Self::Completed),
                "failed" => Ok(Self::Failed),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for CollabAgentToolCallStatus {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for CollabAgentToolCallStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for CollabAgentToolCallStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`CommandAction`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ReadCommandAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"command\","]
    #[doc = "        \"name\","]
    #[doc = "        \"path\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"command\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"name\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"$ref\": \"#/definitions/AbsolutePathBuf\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ReadCommandActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"read\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ListFilesCommandAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"command\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"command\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ListFilesCommandActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"listFiles\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SearchCommandAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"command\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"command\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"query\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"SearchCommandActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"search\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"UnknownCommandAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"command\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"command\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"UnknownCommandActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"unknown\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type")]
    pub enum CommandAction {
        #[doc = "ReadCommandAction"]
        #[serde(rename = "read")]
        Read {
            command: ::std::string::String,
            name: ::std::string::String,
            path: AbsolutePathBuf,
        },
        #[doc = "ListFilesCommandAction"]
        #[serde(rename = "listFiles")]
        ListFiles {
            command: ::std::string::String,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            path: ::std::option::Option<::std::option::Option<::std::string::String>>,
        },
        #[doc = "SearchCommandAction"]
        #[serde(rename = "search")]
        Search {
            command: ::std::string::String,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            path: ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            query: ::std::option::Option<::std::option::Option<::std::string::String>>,
        },
        #[doc = "UnknownCommandAction"]
        #[serde(rename = "unknown")]
        Unknown { command: ::std::string::String },
    }
    #[doc = "`CommandExecutionSource`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"agent\","]
    #[doc = "    \"userShell\","]
    #[doc = "    \"unifiedExecStartup\","]
    #[doc = "    \"unifiedExecInteraction\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum CommandExecutionSource {
        #[serde(rename = "agent")]
        Agent,
        #[serde(rename = "userShell")]
        UserShell,
        #[serde(rename = "unifiedExecStartup")]
        UnifiedExecStartup,
        #[serde(rename = "unifiedExecInteraction")]
        UnifiedExecInteraction,
    }
    impl ::std::fmt::Display for CommandExecutionSource {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Agent => f.write_str("agent"),
                Self::UserShell => f.write_str("userShell"),
                Self::UnifiedExecStartup => f.write_str("unifiedExecStartup"),
                Self::UnifiedExecInteraction => f.write_str("unifiedExecInteraction"),
            }
        }
    }
    impl ::std::str::FromStr for CommandExecutionSource {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "agent" => Ok(Self::Agent),
                "userShell" => Ok(Self::UserShell),
                "unifiedExecStartup" => Ok(Self::UnifiedExecStartup),
                "unifiedExecInteraction" => Ok(Self::UnifiedExecInteraction),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for CommandExecutionSource {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for CommandExecutionSource {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for CommandExecutionSource {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`CommandExecutionStatus`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"inProgress\","]
    #[doc = "    \"completed\","]
    #[doc = "    \"failed\","]
    #[doc = "    \"declined\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum CommandExecutionStatus {
        #[serde(rename = "inProgress")]
        InProgress,
        #[serde(rename = "completed")]
        Completed,
        #[serde(rename = "failed")]
        Failed,
        #[serde(rename = "declined")]
        Declined,
    }
    impl ::std::fmt::Display for CommandExecutionStatus {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::InProgress => f.write_str("inProgress"),
                Self::Completed => f.write_str("completed"),
                Self::Failed => f.write_str("failed"),
                Self::Declined => f.write_str("declined"),
            }
        }
    }
    impl ::std::str::FromStr for CommandExecutionStatus {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "inProgress" => Ok(Self::InProgress),
                "completed" => Ok(Self::Completed),
                "failed" => Ok(Self::Failed),
                "declined" => Ok(Self::Declined),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for CommandExecutionStatus {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for CommandExecutionStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for CommandExecutionStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`DynamicToolCallOutputContentItem`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"InputTextDynamicToolCallOutputContentItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"text\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"text\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"InputTextDynamicToolCallOutputContentItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"inputText\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"InputImageDynamicToolCallOutputContentItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"imageUrl\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"imageUrl\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"InputImageDynamicToolCallOutputContentItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"inputImage\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type")]
    pub enum DynamicToolCallOutputContentItem {
        #[doc = "InputTextDynamicToolCallOutputContentItem"]
        #[serde(rename = "inputText")]
        InputText { text: ::std::string::String },
        #[doc = "InputImageDynamicToolCallOutputContentItem"]
        #[serde(rename = "inputImage")]
        InputImage {
            #[serde(rename = "imageUrl")]
            image_url: ::std::string::String,
        },
    }
    #[doc = "`DynamicToolCallStatus`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"inProgress\","]
    #[doc = "    \"completed\","]
    #[doc = "    \"failed\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum DynamicToolCallStatus {
        #[serde(rename = "inProgress")]
        InProgress,
        #[serde(rename = "completed")]
        Completed,
        #[serde(rename = "failed")]
        Failed,
    }
    impl ::std::fmt::Display for DynamicToolCallStatus {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::InProgress => f.write_str("inProgress"),
                Self::Completed => f.write_str("completed"),
                Self::Failed => f.write_str("failed"),
            }
        }
    }
    impl ::std::str::FromStr for DynamicToolCallStatus {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "inProgress" => Ok(Self::InProgress),
                "completed" => Ok(Self::Completed),
                "failed" => Ok(Self::Failed),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for DynamicToolCallStatus {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for DynamicToolCallStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for DynamicToolCallStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`FileUpdateChange`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"diff\","]
    #[doc = "    \"kind\","]
    #[doc = "    \"path\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"diff\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"kind\": {"]
    #[doc = "      \"$ref\": \"#/definitions/PatchChangeKind\""]
    #[doc = "    },"]
    #[doc = "    \"path\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct FileUpdateChange {
        pub diff: ::std::string::String,
        pub kind: PatchChangeKind,
        pub path: ::std::string::String,
    }
    #[doc = "`HookPromptFragment`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"hookRunId\","]
    #[doc = "    \"text\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"hookRunId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"text\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct HookPromptFragment {
        #[serde(rename = "hookRunId")]
        pub hook_run_id: ::std::string::String,
        pub text: ::std::string::String,
    }
    #[doc = "`ImageDetail`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"auto\","]
    #[doc = "    \"low\","]
    #[doc = "    \"high\","]
    #[doc = "    \"original\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum ImageDetail {
        #[serde(rename = "auto")]
        Auto,
        #[serde(rename = "low")]
        Low,
        #[serde(rename = "high")]
        High,
        #[serde(rename = "original")]
        Original,
    }
    impl ::std::fmt::Display for ImageDetail {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Auto => f.write_str("auto"),
                Self::Low => f.write_str("low"),
                Self::High => f.write_str("high"),
                Self::Original => f.write_str("original"),
            }
        }
    }
    impl ::std::str::FromStr for ImageDetail {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "auto" => Ok(Self::Auto),
                "low" => Ok(Self::Low),
                "high" => Ok(Self::High),
                "original" => Ok(Self::Original),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for ImageDetail {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for ImageDetail {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for ImageDetail {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`LegacyAppPathString`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    #[serde(transparent)]
    pub struct LegacyAppPathString(pub ::std::string::String);
    impl ::std::ops::Deref for LegacyAppPathString {
        type Target = ::std::string::String;
        fn deref(&self) -> &::std::string::String {
            &self.0
        }
    }
    impl ::std::convert::From<LegacyAppPathString> for ::std::string::String {
        fn from(value: LegacyAppPathString) -> Self {
            value.0
        }
    }
    impl ::std::convert::From<::std::string::String> for LegacyAppPathString {
        fn from(value: ::std::string::String) -> Self {
            Self(value)
        }
    }
    impl ::std::str::FromStr for LegacyAppPathString {
        type Err = ::std::convert::Infallible;
        fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
            Ok(Self(value.to_string()))
        }
    }
    impl ::std::fmt::Display for LegacyAppPathString {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            self.0.fmt(f)
        }
    }
    #[doc = "`McpToolCallAppContext`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"connectorId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"actionName\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"appName\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"connectorId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"linkId\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"resourceUri\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"templateId\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct McpToolCallAppContext {
        #[serde(
            rename = "actionName",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub action_name: ::std::option::Option<::std::option::Option<::std::string::String>>,
        #[serde(
            rename = "appName",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub app_name: ::std::option::Option<::std::option::Option<::std::string::String>>,
        #[serde(rename = "connectorId")]
        pub connector_id: ::std::string::String,
        #[serde(
            rename = "linkId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub link_id: ::std::option::Option<::std::option::Option<::std::string::String>>,
        #[serde(
            rename = "resourceUri",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub resource_uri: ::std::option::Option<::std::option::Option<::std::string::String>>,
        #[serde(
            rename = "templateId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub template_id: ::std::option::Option<::std::option::Option<::std::string::String>>,
    }
    #[doc = "`McpToolCallError`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"message\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"message\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct McpToolCallError {
        pub message: ::std::string::String,
    }
    #[doc = "`McpToolCallResult`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"content\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"_meta\": true,"]
    #[doc = "    \"content\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": true"]
    #[doc = "    },"]
    #[doc = "    \"structuredContent\": true"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct McpToolCallResult {
        pub content: ::std::vec::Vec<::serde_json::Value>,
        #[serde(
            rename = "_meta",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub meta: ::std::option::Option<::serde_json::Value>,
        #[serde(
            rename = "structuredContent",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub structured_content: ::std::option::Option<::serde_json::Value>,
    }
    #[doc = "`McpToolCallStatus`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"inProgress\","]
    #[doc = "    \"completed\","]
    #[doc = "    \"failed\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum McpToolCallStatus {
        #[serde(rename = "inProgress")]
        InProgress,
        #[serde(rename = "completed")]
        Completed,
        #[serde(rename = "failed")]
        Failed,
    }
    impl ::std::fmt::Display for McpToolCallStatus {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::InProgress => f.write_str("inProgress"),
                Self::Completed => f.write_str("completed"),
                Self::Failed => f.write_str("failed"),
            }
        }
    }
    impl ::std::str::FromStr for McpToolCallStatus {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "inProgress" => Ok(Self::InProgress),
                "completed" => Ok(Self::Completed),
                "failed" => Ok(Self::Failed),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for McpToolCallStatus {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for McpToolCallStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for McpToolCallStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`MemoryCitation`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"entries\","]
    #[doc = "    \"threadIds\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"entries\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"$ref\": \"#/definitions/MemoryCitationEntry\""]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    \"threadIds\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"type\": \"string\""]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct MemoryCitation {
        pub entries: ::std::vec::Vec<MemoryCitationEntry>,
        #[serde(rename = "threadIds")]
        pub thread_ids: ::std::vec::Vec<::std::string::String>,
    }
    #[doc = "`MemoryCitationEntry`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"lineEnd\","]
    #[doc = "    \"lineStart\","]
    #[doc = "    \"note\","]
    #[doc = "    \"path\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"lineEnd\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"uint32\","]
    #[doc = "      \"minimum\": 0.0"]
    #[doc = "    },"]
    #[doc = "    \"lineStart\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"uint32\","]
    #[doc = "      \"minimum\": 0.0"]
    #[doc = "    },"]
    #[doc = "    \"note\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"path\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct MemoryCitationEntry {
        #[serde(rename = "lineEnd")]
        pub line_end: u32,
        #[serde(rename = "lineStart")]
        pub line_start: u32,
        pub note: ::std::string::String,
        pub path: ::std::string::String,
    }
    #[doc = "Classifies an assistant message as interim commentary or final answer text.\n\nProviders do not emit this consistently, so callers must treat `None` as \"phase unknown\" and keep compatibility behavior for legacy models."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"Classifies an assistant message as interim commentary or final answer text.\\n\\nProviders do not emit this consistently, so callers must treat `None` as \\\"phase unknown\\\" and keep compatibility behavior for legacy models.\","]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"description\": \"Mid-turn assistant text (for example preamble/progress narration).\\n\\nAdditional tool calls or assistant output may follow before turn completion.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"commentary\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"description\": \"The assistant's terminal answer text for the current turn.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"final_answer\""]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum MessagePhase {
        #[doc = "Mid-turn assistant text (for example preamble/progress narration).\n\nAdditional tool calls or assistant output may follow before turn completion."]
        #[serde(rename = "commentary")]
        Commentary,
        #[doc = "The assistant's terminal answer text for the current turn."]
        #[serde(rename = "final_answer")]
        FinalAnswer,
    }
    impl ::std::fmt::Display for MessagePhase {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Commentary => f.write_str("commentary"),
                Self::FinalAnswer => f.write_str("final_answer"),
            }
        }
    }
    impl ::std::str::FromStr for MessagePhase {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "commentary" => Ok(Self::Commentary),
                "final_answer" => Ok(Self::FinalAnswer),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for MessagePhase {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for MessagePhase {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for MessagePhase {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`PatchApplyStatus`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"inProgress\","]
    #[doc = "    \"completed\","]
    #[doc = "    \"failed\","]
    #[doc = "    \"declined\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum PatchApplyStatus {
        #[serde(rename = "inProgress")]
        InProgress,
        #[serde(rename = "completed")]
        Completed,
        #[serde(rename = "failed")]
        Failed,
        #[serde(rename = "declined")]
        Declined,
    }
    impl ::std::fmt::Display for PatchApplyStatus {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::InProgress => f.write_str("inProgress"),
                Self::Completed => f.write_str("completed"),
                Self::Failed => f.write_str("failed"),
                Self::Declined => f.write_str("declined"),
            }
        }
    }
    impl ::std::str::FromStr for PatchApplyStatus {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "inProgress" => Ok(Self::InProgress),
                "completed" => Ok(Self::Completed),
                "failed" => Ok(Self::Failed),
                "declined" => Ok(Self::Declined),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for PatchApplyStatus {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for PatchApplyStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for PatchApplyStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`PatchChangeKind`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"AddPatchChangeKind\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"AddPatchChangeKindType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"add\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"DeletePatchChangeKind\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"DeletePatchChangeKindType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"delete\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"UpdatePatchChangeKind\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"move_path\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"UpdatePatchChangeKindType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"update\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type")]
    pub enum PatchChangeKind {
        #[serde(rename = "add")]
        Add,
        #[serde(rename = "delete")]
        Delete,
        #[doc = "UpdatePatchChangeKind"]
        #[serde(rename = "update")]
        Update {
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            move_path: ::std::option::Option<::std::string::String>,
        },
    }
    #[doc = "A non-empty reasoning effort value advertised by the model."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"A non-empty reasoning effort value advertised by the model.\","]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"minLength\": 1"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    #[serde(transparent)]
    pub struct ReasoningEffort(::std::string::String);
    impl ::std::ops::Deref for ReasoningEffort {
        type Target = ::std::string::String;
        fn deref(&self) -> &::std::string::String {
            &self.0
        }
    }
    impl ::std::convert::From<ReasoningEffort> for ::std::string::String {
        fn from(value: ReasoningEffort) -> Self {
            value.0
        }
    }
    impl ::std::str::FromStr for ReasoningEffort {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            if value.chars().count() < 1usize {
                return Err("shorter than 1 characters".into());
            }
            Ok(Self(value.to_string()))
        }
    }
    impl ::std::convert::TryFrom<&str> for ReasoningEffort {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for ReasoningEffort {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for ReasoningEffort {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl<'de> ::serde::Deserialize<'de> for ReasoningEffort {
        fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
        where
            D: ::serde::Deserializer<'de>,
        {
            ::std::string::String::deserialize(deserializer)?
                .parse()
                .map_err(|e: self::error::ConversionError| {
                    <D::Error as ::serde::de::Error>::custom(e.to_string())
                })
        }
    }
    #[doc = "`SubAgentActivityKind`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"started\","]
    #[doc = "    \"interacted\","]
    #[doc = "    \"interrupted\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum SubAgentActivityKind {
        #[serde(rename = "started")]
        Started,
        #[serde(rename = "interacted")]
        Interacted,
        #[serde(rename = "interrupted")]
        Interrupted,
    }
    impl ::std::fmt::Display for SubAgentActivityKind {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Started => f.write_str("started"),
                Self::Interacted => f.write_str("interacted"),
                Self::Interrupted => f.write_str("interrupted"),
            }
        }
    }
    impl ::std::str::FromStr for SubAgentActivityKind {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "started" => Ok(Self::Started),
                "interacted" => Ok(Self::Interacted),
                "interrupted" => Ok(Self::Interrupted),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for SubAgentActivityKind {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for SubAgentActivityKind {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for SubAgentActivityKind {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`TextElement`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"byteRange\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"byteRange\": {"]
    #[doc = "      \"description\": \"Byte range in the parent `text` buffer that this element occupies.\","]
    #[doc = "      \"allOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/ByteRange\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"placeholder\": {"]
    #[doc = "      \"description\": \"Optional human-readable placeholder for the element, displayed in the UI.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Optional human-readable placeholder for the element, displayed in the UI.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct TextElement {
        #[doc = "Byte range in the parent `text` buffer that this element occupies."]
        #[serde(rename = "byteRange")]
        pub byte_range: ByteRange,
        #[doc = "Optional human-readable placeholder for the element, displayed in the UI."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub placeholder: ::std::option::Option<::std::option::Option<::std::string::String>>,
    }
    #[doc = "`ThreadItem`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"UserMessageThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"content\","]
    #[doc = "        \"id\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"clientId\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"content\": {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/UserInput\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"UserMessageThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"userMessage\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"HookPromptThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"fragments\","]
    #[doc = "        \"id\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"fragments\": {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/HookPromptFragment\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"HookPromptThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"hookPrompt\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"AgentMessageThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"id\","]
    #[doc = "        \"text\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"memoryCitation\": {"]
    #[doc = "          \"default\": null,"]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/MemoryCitation\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"phase\": {"]
    #[doc = "          \"default\": null,"]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/MessagePhase\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"text\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"AgentMessageThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"agentMessage\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"PlanThreadItem\","]
    #[doc = "      \"description\": \"EXPERIMENTAL - proposed plan item content. The completed plan item is authoritative and may not match the concatenation of `PlanDelta` text.\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"id\","]
    #[doc = "        \"text\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"text\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"PlanThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"plan\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ReasoningThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"id\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"content\": {"]
    #[doc = "          \"default\": [],"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"type\": \"string\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"summary\": {"]
    #[doc = "          \"default\": [],"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"type\": \"string\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ReasoningThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"reasoning\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"CommandExecutionThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"command\","]
    #[doc = "        \"commandActions\","]
    #[doc = "        \"cwd\","]
    #[doc = "        \"id\","]
    #[doc = "        \"status\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"aggregatedOutput\": {"]
    #[doc = "          \"description\": \"The command's output, aggregated from stdout and stderr.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"The command's output, aggregated from stdout and stderr.\","]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"command\": {"]
    #[doc = "          \"description\": \"The command to be executed.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"commandActions\": {"]
    #[doc = "          \"description\": \"A best-effort parsing of the command to understand the action(s) it will perform. This returns a list of CommandAction objects because a single shell command may be composed of many commands piped together.\","]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/CommandAction\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"cwd\": {"]
    #[doc = "          \"description\": \"The command's working directory.\","]
    #[doc = "          \"allOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/LegacyAppPathString\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"durationMs\": {"]
    #[doc = "          \"description\": \"The duration of the command execution in milliseconds.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"The duration of the command execution in milliseconds.\","]
    #[doc = "              \"type\": \"integer\","]
    #[doc = "              \"format\": \"int64\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"exitCode\": {"]
    #[doc = "          \"description\": \"The command's exit code.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"The command's exit code.\","]
    #[doc = "              \"type\": \"integer\","]
    #[doc = "              \"format\": \"int32\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"processId\": {"]
    #[doc = "          \"description\": \"Identifier for the underlying PTY process (when available).\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"Identifier for the underlying PTY process (when available).\","]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"source\": {"]
    #[doc = "          \"default\": \"agent\","]
    #[doc = "          \"allOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/CommandExecutionSource\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"status\": {"]
    #[doc = "          \"$ref\": \"#/definitions/CommandExecutionStatus\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"CommandExecutionThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"commandExecution\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"FileChangeThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"changes\","]
    #[doc = "        \"id\","]
    #[doc = "        \"status\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"changes\": {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/FileUpdateChange\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"status\": {"]
    #[doc = "          \"$ref\": \"#/definitions/PatchApplyStatus\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"FileChangeThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"fileChange\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"McpToolCallThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"arguments\","]
    #[doc = "        \"id\","]
    #[doc = "        \"server\","]
    #[doc = "        \"status\","]
    #[doc = "        \"tool\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"appContext\": {"]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/McpToolCallAppContext\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"arguments\": true,"]
    #[doc = "        \"durationMs\": {"]
    #[doc = "          \"description\": \"The duration of the MCP tool call in milliseconds.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"The duration of the MCP tool call in milliseconds.\","]
    #[doc = "              \"type\": \"integer\","]
    #[doc = "              \"format\": \"int64\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"error\": {"]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/McpToolCallError\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"mcpAppResourceUri\": {"]
    #[doc = "          \"description\": \"Deprecated: use `appContext.resourceUri` instead.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"Deprecated: use `appContext.resourceUri` instead.\","]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"pluginId\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"result\": {"]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/McpToolCallResult\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"server\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"status\": {"]
    #[doc = "          \"$ref\": \"#/definitions/McpToolCallStatus\""]
    #[doc = "        },"]
    #[doc = "        \"tool\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"McpToolCallThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"mcpToolCall\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"DynamicToolCallThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"arguments\","]
    #[doc = "        \"id\","]
    #[doc = "        \"status\","]
    #[doc = "        \"tool\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"arguments\": true,"]
    #[doc = "        \"contentItems\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"array\","]
    #[doc = "              \"items\": {"]
    #[doc = "                \"$ref\": \"#/definitions/DynamicToolCallOutputContentItem\""]
    #[doc = "              }"]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"durationMs\": {"]
    #[doc = "          \"description\": \"The duration of the dynamic tool call in milliseconds.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"The duration of the dynamic tool call in milliseconds.\","]
    #[doc = "              \"type\": \"integer\","]
    #[doc = "              \"format\": \"int64\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"namespace\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"status\": {"]
    #[doc = "          \"$ref\": \"#/definitions/DynamicToolCallStatus\""]
    #[doc = "        },"]
    #[doc = "        \"success\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"boolean\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"tool\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"DynamicToolCallThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"dynamicToolCall\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"CollabAgentToolCallThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"agentsStates\","]
    #[doc = "        \"id\","]
    #[doc = "        \"receiverThreadIds\","]
    #[doc = "        \"senderThreadId\","]
    #[doc = "        \"status\","]
    #[doc = "        \"tool\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"agentsStates\": {"]
    #[doc = "          \"description\": \"Last known status of the target agents, when available.\","]
    #[doc = "          \"type\": \"object\","]
    #[doc = "          \"additionalProperties\": {"]
    #[doc = "            \"$ref\": \"#/definitions/CollabAgentState\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"description\": \"Unique identifier for this collab tool call.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"model\": {"]
    #[doc = "          \"description\": \"Model requested for the spawned agent, when applicable.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"Model requested for the spawned agent, when applicable.\","]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"prompt\": {"]
    #[doc = "          \"description\": \"Prompt text sent as part of the collab tool call, when available.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"Prompt text sent as part of the collab tool call, when available.\","]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"reasoningEffort\": {"]
    #[doc = "          \"description\": \"Reasoning effort requested for the spawned agent, when applicable.\","]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/ReasoningEffort\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"receiverThreadIds\": {"]
    #[doc = "          \"description\": \"Thread ID of the receiving agent, when applicable. In case of spawn operation, this corresponds to the newly spawned agent.\","]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"type\": \"string\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"senderThreadId\": {"]
    #[doc = "          \"description\": \"Thread ID of the agent issuing the collab request.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"status\": {"]
    #[doc = "          \"description\": \"Current status of the collab tool call.\","]
    #[doc = "          \"allOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/CollabAgentToolCallStatus\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"tool\": {"]
    #[doc = "          \"description\": \"Name of the collab tool that was invoked.\","]
    #[doc = "          \"allOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/CollabAgentTool\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"CollabAgentToolCallThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"collabAgentToolCall\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SubAgentActivityThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"agentPath\","]
    #[doc = "        \"agentThreadId\","]
    #[doc = "        \"id\","]
    #[doc = "        \"kind\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"agentPath\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"agentThreadId\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"$ref\": \"#/definitions/SubAgentActivityKind\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"SubAgentActivityThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"subAgentActivity\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"WebSearchThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"id\","]
    #[doc = "        \"query\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"action\": {"]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/WebSearchAction\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"query\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"WebSearchThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"webSearch\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ImageViewThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"id\","]
    #[doc = "        \"path\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"$ref\": \"#/definitions/LegacyAppPathString\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ImageViewThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"imageView\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SleepThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"durationMs\","]
    #[doc = "        \"id\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"durationMs\": {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"uint64\","]
    #[doc = "          \"minimum\": 0.0"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"SleepThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"sleep\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ImageGenerationThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"id\","]
    #[doc = "        \"result\","]
    #[doc = "        \"status\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"result\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"revisedPrompt\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"savedPath\": {"]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/AbsolutePathBuf\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"status\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ImageGenerationThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"imageGeneration\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"EnteredReviewModeThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"id\","]
    #[doc = "        \"review\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"review\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"EnteredReviewModeThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"enteredReviewMode\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ExitedReviewModeThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"id\","]
    #[doc = "        \"review\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"review\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ExitedReviewModeThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"exitedReviewMode\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ContextCompactionThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"id\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ContextCompactionThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"contextCompaction\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type")]
    pub enum ThreadItem {
        #[doc = "UserMessageThreadItem"]
        #[serde(rename = "userMessage")]
        UserMessage {
            #[serde(
                rename = "clientId",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            client_id: ::std::option::Option<::std::option::Option<::std::string::String>>,
            content: ::std::vec::Vec<UserInput>,
            id: ::std::string::String,
        },
        #[doc = "HookPromptThreadItem"]
        #[serde(rename = "hookPrompt")]
        HookPrompt {
            fragments: ::std::vec::Vec<HookPromptFragment>,
            id: ::std::string::String,
        },
        #[doc = "AgentMessageThreadItem"]
        #[serde(rename = "agentMessage")]
        AgentMessage {
            id: ::std::string::String,
            #[serde(
                rename = "memoryCitation",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            memory_citation: ::std::option::Option<::std::option::Option<MemoryCitation>>,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            phase: ::std::option::Option<::std::option::Option<MessagePhase>>,
            text: ::std::string::String,
        },
        #[doc = "PlanThreadItem\n\nEXPERIMENTAL - proposed plan item content. The completed plan item is authoritative and may not match the concatenation of `PlanDelta` text."]
        #[serde(rename = "plan")]
        Plan {
            id: ::std::string::String,
            text: ::std::string::String,
        },
        #[doc = "ReasoningThreadItem"]
        #[serde(rename = "reasoning")]
        Reasoning {
            #[serde(default)]
            content: ::std::vec::Vec<::std::string::String>,
            id: ::std::string::String,
            #[serde(default)]
            summary: ::std::vec::Vec<::std::string::String>,
        },
        #[doc = "CommandExecutionThreadItem"]
        #[serde(rename = "commandExecution")]
        CommandExecution {
            #[doc = "The command's output, aggregated from stdout and stderr."]
            #[serde(
                rename = "aggregatedOutput",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            aggregated_output: ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[doc = "The command to be executed."]
            command: ::std::string::String,
            #[doc = "A best-effort parsing of the command to understand the action(s) it will perform. This returns a list of CommandAction objects because a single shell command may be composed of many commands piped together."]
            #[serde(rename = "commandActions")]
            command_actions: ::std::vec::Vec<CommandAction>,
            #[doc = "The command's working directory."]
            cwd: LegacyAppPathString,
            #[doc = "The duration of the command execution in milliseconds."]
            #[serde(
                rename = "durationMs",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            #[ts(type = "number | null")]
            duration_ms: ::std::option::Option<::std::option::Option<i64>>,
            #[doc = "The command's exit code."]
            #[serde(
                rename = "exitCode",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            exit_code: ::std::option::Option<::std::option::Option<i32>>,
            id: ::std::string::String,
            #[doc = "Identifier for the underlying PTY process (when available)."]
            #[serde(
                rename = "processId",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            process_id: ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[serde(default = "defaults::thread_item_command_execution_source")]
            source: CommandExecutionSource,
            status: CommandExecutionStatus,
        },
        #[doc = "FileChangeThreadItem"]
        #[serde(rename = "fileChange")]
        FileChange {
            changes: ::std::vec::Vec<FileUpdateChange>,
            id: ::std::string::String,
            status: PatchApplyStatus,
        },
        #[doc = "McpToolCallThreadItem"]
        #[serde(rename = "mcpToolCall")]
        McpToolCall {
            #[serde(
                rename = "appContext",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            app_context: ::std::option::Option<::std::option::Option<McpToolCallAppContext>>,
            arguments: ::serde_json::Value,
            #[doc = "The duration of the MCP tool call in milliseconds."]
            #[serde(
                rename = "durationMs",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            #[ts(type = "number | null")]
            duration_ms: ::std::option::Option<::std::option::Option<i64>>,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            error: ::std::option::Option<::std::option::Option<McpToolCallError>>,
            id: ::std::string::String,
            #[doc = "Deprecated: use `appContext.resourceUri` instead."]
            #[serde(
                rename = "mcpAppResourceUri",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            mcp_app_resource_uri:
                ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[serde(
                rename = "pluginId",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            plugin_id: ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            result: ::std::option::Option<::std::option::Option<McpToolCallResult>>,
            server: ::std::string::String,
            status: McpToolCallStatus,
            tool: ::std::string::String,
        },
        #[doc = "DynamicToolCallThreadItem"]
        #[serde(rename = "dynamicToolCall")]
        DynamicToolCall {
            arguments: ::serde_json::Value,
            #[serde(
                rename = "contentItems",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            content_items: ::std::option::Option<
                ::std::option::Option<::std::vec::Vec<DynamicToolCallOutputContentItem>>,
            >,
            #[doc = "The duration of the dynamic tool call in milliseconds."]
            #[serde(
                rename = "durationMs",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            #[ts(type = "number | null")]
            duration_ms: ::std::option::Option<::std::option::Option<i64>>,
            id: ::std::string::String,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            namespace: ::std::option::Option<::std::option::Option<::std::string::String>>,
            status: DynamicToolCallStatus,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            success: ::std::option::Option<::std::option::Option<bool>>,
            tool: ::std::string::String,
        },
        #[doc = "CollabAgentToolCallThreadItem"]
        #[serde(rename = "collabAgentToolCall")]
        CollabAgentToolCall {
            #[doc = "Last known status of the target agents, when available."]
            #[serde(rename = "agentsStates")]
            agents_states: ::std::collections::HashMap<::std::string::String, CollabAgentState>,
            #[doc = "Unique identifier for this collab tool call."]
            id: ::std::string::String,
            #[doc = "Model requested for the spawned agent, when applicable."]
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            model: ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[doc = "Prompt text sent as part of the collab tool call, when available."]
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            prompt: ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[doc = "Reasoning effort requested for the spawned agent, when applicable."]
            #[serde(
                rename = "reasoningEffort",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            reasoning_effort: ::std::option::Option<::std::option::Option<ReasoningEffort>>,
            #[doc = "Thread ID of the receiving agent, when applicable. In case of spawn operation, this corresponds to the newly spawned agent."]
            #[serde(rename = "receiverThreadIds")]
            receiver_thread_ids: ::std::vec::Vec<::std::string::String>,
            #[doc = "Thread ID of the agent issuing the collab request."]
            #[serde(rename = "senderThreadId")]
            sender_thread_id: ::std::string::String,
            #[doc = "Current status of the collab tool call."]
            status: CollabAgentToolCallStatus,
            #[doc = "Name of the collab tool that was invoked."]
            tool: CollabAgentTool,
        },
        #[doc = "SubAgentActivityThreadItem"]
        #[serde(rename = "subAgentActivity")]
        SubAgentActivity {
            #[serde(rename = "agentPath")]
            agent_path: ::std::string::String,
            #[serde(rename = "agentThreadId")]
            agent_thread_id: ::std::string::String,
            id: ::std::string::String,
            kind: SubAgentActivityKind,
        },
        #[doc = "WebSearchThreadItem"]
        #[serde(rename = "webSearch")]
        WebSearch {
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            action: ::std::option::Option<::std::option::Option<WebSearchAction>>,
            id: ::std::string::String,
            query: ::std::string::String,
        },
        #[doc = "ImageViewThreadItem"]
        #[serde(rename = "imageView")]
        ImageView {
            id: ::std::string::String,
            path: LegacyAppPathString,
        },
        #[doc = "SleepThreadItem"]
        #[serde(rename = "sleep")]
        Sleep {
            #[serde(rename = "durationMs")]
            #[ts(type = "number")]
            duration_ms: u64,
            id: ::std::string::String,
        },
        #[doc = "ImageGenerationThreadItem"]
        #[serde(rename = "imageGeneration")]
        ImageGeneration {
            id: ::std::string::String,
            result: ::std::string::String,
            #[serde(
                rename = "revisedPrompt",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            revised_prompt: ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[serde(
                rename = "savedPath",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            saved_path: ::std::option::Option<::std::option::Option<AbsolutePathBuf>>,
            status: ::std::string::String,
        },
        #[doc = "EnteredReviewModeThreadItem"]
        #[serde(rename = "enteredReviewMode")]
        EnteredReviewMode {
            id: ::std::string::String,
            review: ::std::string::String,
        },
        #[doc = "ExitedReviewModeThreadItem"]
        #[serde(rename = "exitedReviewMode")]
        ExitedReviewMode {
            id: ::std::string::String,
            review: ::std::string::String,
        },
        #[doc = "ContextCompactionThreadItem"]
        #[serde(rename = "contextCompaction")]
        ContextCompaction { id: ::std::string::String },
    }
    #[doc = "`UserInput`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"TextUserInput\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"text\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"text\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"text_elements\": {"]
    #[doc = "          \"description\": \"UI-defined spans within `text` used to render or persist special elements.\","]
    #[doc = "          \"default\": [],"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/TextElement\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"TextUserInputType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"text\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ImageUserInput\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\","]
    #[doc = "        \"url\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"detail\": {"]
    #[doc = "          \"default\": null,"]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/ImageDetail\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ImageUserInputType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"image\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"url\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"LocalImageUserInput\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"path\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"detail\": {"]
    #[doc = "          \"default\": null,"]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/ImageDetail\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"LocalImageUserInputType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"localImage\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SkillUserInput\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"name\","]
    #[doc = "        \"path\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"name\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"SkillUserInputType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"skill\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"MentionUserInput\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"name\","]
    #[doc = "        \"path\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"name\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"MentionUserInputType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"mention\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type")]
    pub enum UserInput {
        #[doc = "TextUserInput"]
        #[serde(rename = "text")]
        Text {
            text: ::std::string::String,
            #[doc = "UI-defined spans within `text` used to render or persist special elements."]
            #[serde(default)]
            text_elements: ::std::vec::Vec<TextElement>,
        },
        #[doc = "ImageUserInput"]
        #[serde(rename = "image")]
        Image {
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            detail: ::std::option::Option<::std::option::Option<ImageDetail>>,
            url: ::std::string::String,
        },
        #[doc = "LocalImageUserInput"]
        #[serde(rename = "localImage")]
        LocalImage {
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            detail: ::std::option::Option<::std::option::Option<ImageDetail>>,
            path: ::std::string::String,
        },
        #[doc = "SkillUserInput"]
        #[serde(rename = "skill")]
        Skill {
            name: ::std::string::String,
            path: ::std::string::String,
        },
        #[doc = "MentionUserInput"]
        #[serde(rename = "mention")]
        Mention {
            name: ::std::string::String,
            path: ::std::string::String,
        },
    }
    #[doc = "`WebSearchAction`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SearchWebSearchAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"queries\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"array\","]
    #[doc = "              \"items\": {"]
    #[doc = "                \"type\": \"string\""]
    #[doc = "              }"]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"query\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"SearchWebSearchActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"search\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"OpenPageWebSearchAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"OpenPageWebSearchActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"openPage\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"url\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"FindInPageWebSearchAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"pattern\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"FindInPageWebSearchActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"findInPage\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"url\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"OtherWebSearchAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"OtherWebSearchActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"other\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type")]
    pub enum WebSearchAction {
        #[doc = "SearchWebSearchAction"]
        #[serde(rename = "search")]
        Search {
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            queries: ::std::option::Option<
                ::std::option::Option<::std::vec::Vec<::std::string::String>>,
            >,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            query: ::std::option::Option<::std::option::Option<::std::string::String>>,
        },
        #[doc = "OpenPageWebSearchAction"]
        #[serde(rename = "openPage")]
        OpenPage {
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            url: ::std::option::Option<::std::option::Option<::std::string::String>>,
        },
        #[doc = "FindInPageWebSearchAction"]
        #[serde(rename = "findInPage")]
        FindInPage {
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            pattern: ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            url: ::std::option::Option<::std::option::Option<::std::string::String>>,
        },
        #[serde(rename = "other")]
        Other,
    }
    #[doc = r" Generation of default values for serde."]
    pub mod defaults {
        pub(super) fn thread_item_command_execution_source() -> super::CommandExecutionSource {
            super::CommandExecutionSource::Agent
        }
    }
}
pub mod server_notification {
    #[doc = r" Error types."]
    pub mod error {
        #[doc = r" Error from a `TryFrom` or `FromStr` implementation."]
        pub struct ConversionError(::std::borrow::Cow<'static, str>);
        impl ::std::error::Error for ConversionError {}
        impl ::std::fmt::Display for ConversionError {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                ::std::fmt::Display::fmt(&self.0, f)
            }
        }
        impl ::std::fmt::Debug for ConversionError {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                ::std::fmt::Debug::fmt(&self.0, f)
            }
        }
        impl From<&'static str> for ConversionError {
            fn from(value: &'static str) -> Self {
                Self(value.into())
            }
        }
        impl From<String> for ConversionError {
            fn from(value: String) -> Self {
                Self(value.into())
            }
        }
    }
    #[doc = "A path that is guaranteed to be absolute and normalized (though it is not guaranteed to be canonicalized or exist on the filesystem).\n\nIMPORTANT: When deserializing an `AbsolutePathBuf`, a base path must be set using [AbsolutePathBufGuard::new]. If no base path is set, the deserialization will fail unless the path being deserialized is already absolute."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"A path that is guaranteed to be absolute and normalized (though it is not guaranteed to be canonicalized or exist on the filesystem).\\n\\nIMPORTANT: When deserializing an `AbsolutePathBuf`, a base path must be set using [AbsolutePathBufGuard::new]. If no base path is set, the deserialization will fail unless the path being deserialized is already absolute.\","]
    #[doc = "  \"type\": \"string\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    #[serde(transparent)]
    pub struct AbsolutePathBuf(pub ::std::string::String);
    impl ::std::ops::Deref for AbsolutePathBuf {
        type Target = ::std::string::String;
        fn deref(&self) -> &::std::string::String {
            &self.0
        }
    }
    impl ::std::convert::From<AbsolutePathBuf> for ::std::string::String {
        fn from(value: AbsolutePathBuf) -> Self {
            value.0
        }
    }
    impl ::std::convert::From<::std::string::String> for AbsolutePathBuf {
        fn from(value: ::std::string::String) -> Self {
            Self(value)
        }
    }
    impl ::std::str::FromStr for AbsolutePathBuf {
        type Err = ::std::convert::Infallible;
        fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
            Ok(Self(value.to_string()))
        }
    }
    impl ::std::fmt::Display for AbsolutePathBuf {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            self.0.fmt(f)
        }
    }
    #[doc = "`AccountLoginCompletedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"AccountLoginCompletedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"success\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"error\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"loginId\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"success\": {"]
    #[doc = "      \"type\": \"boolean\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct AccountLoginCompletedNotification {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub error: ::std::option::Option<::std::string::String>,
        #[serde(
            rename = "loginId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub login_id: ::std::option::Option<::std::string::String>,
        pub success: bool,
    }
    #[doc = "Sparse rolling rate-limit update.\n\nClients should merge available values into the most recent `account/rateLimits/read` response or refetch that snapshot. Nullable account metadata may be unavailable in a rolling update and does not clear a previously observed value."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"AccountRateLimitsUpdatedNotification\","]
    #[doc = "  \"description\": \"Sparse rolling rate-limit update.\\n\\nClients should merge available values into the most recent `account/rateLimits/read` response or refetch that snapshot. Nullable account metadata may be unavailable in a rolling update and does not clear a previously observed value.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"rateLimits\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"rateLimits\": {"]
    #[doc = "      \"$ref\": \"#/definitions/RateLimitSnapshot\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct AccountRateLimitsUpdatedNotification {
        #[serde(rename = "rateLimits")]
        pub rate_limits: RateLimitSnapshot,
    }
    #[doc = "`AccountUpdatedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"AccountUpdatedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"authMode\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/AuthMode\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"planType\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/PlanType\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct AccountUpdatedNotification {
        #[serde(
            rename = "authMode",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub auth_mode: ::std::option::Option<AuthMode>,
        #[serde(
            rename = "planType",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub plan_type: ::std::option::Option<PlanType>,
    }
    impl ::std::default::Default for AccountUpdatedNotification {
        fn default() -> Self {
            Self {
                auth_mode: Default::default(),
                plan_type: Default::default(),
            }
        }
    }
    #[doc = "`ActivePermissionProfile`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"id\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"extends\": {"]
    #[doc = "      \"description\": \"Parent profile identifier from the selected permissions profile's `extends` setting, when present.\","]
    #[doc = "      \"default\": null,"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Parent profile identifier from the selected permissions profile's `extends` setting, when present.\","]
    #[doc = "          \"default\": null,"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"id\": {"]
    #[doc = "      \"description\": \"Identifier from `default_permissions` or the implicit built-in default, such as `:workspace` or a user-defined `[permissions.<id>]` profile.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ActivePermissionProfile {
        #[doc = "Parent profile identifier from the selected permissions profile's `extends` setting, when present."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub extends: ::std::option::Option<::std::string::String>,
        #[doc = "Identifier from `default_permissions` or the implicit built-in default, such as `:workspace` or a user-defined `[permissions.<id>]` profile."]
        pub id: ::std::string::String,
    }
    #[doc = "`AdditionalFileSystemPermissions`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"entries\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/FileSystemSandboxEntry\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"globScanMaxDepth\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"uint\","]
    #[doc = "          \"minimum\": 1.0"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"read\": {"]
    #[doc = "      \"description\": \"This will be removed in favor of `entries`.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"This will be removed in favor of `entries`.\","]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/LegacyAppPathString\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"write\": {"]
    #[doc = "      \"description\": \"This will be removed in favor of `entries`.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"This will be removed in favor of `entries`.\","]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/LegacyAppPathString\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct AdditionalFileSystemPermissions {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub entries:
            ::std::option::Option<::std::option::Option<::std::vec::Vec<FileSystemSandboxEntry>>>,
        #[serde(
            rename = "globScanMaxDepth",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub glob_scan_max_depth:
            ::std::option::Option<::std::option::Option<::std::num::NonZeroU32>>,
        #[doc = "This will be removed in favor of `entries`."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub read:
            ::std::option::Option<::std::option::Option<::std::vec::Vec<LegacyAppPathString>>>,
        #[doc = "This will be removed in favor of `entries`."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub write:
            ::std::option::Option<::std::option::Option<::std::vec::Vec<LegacyAppPathString>>>,
    }
    impl ::std::default::Default for AdditionalFileSystemPermissions {
        fn default() -> Self {
            Self {
                entries: Default::default(),
                glob_scan_max_depth: Default::default(),
                read: Default::default(),
                write: Default::default(),
            }
        }
    }
    #[doc = "`AdditionalNetworkPermissions`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"enabled\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"boolean\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct AdditionalNetworkPermissions {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub enabled: ::std::option::Option<::std::option::Option<bool>>,
    }
    impl ::std::default::Default for AdditionalNetworkPermissions {
        fn default() -> Self {
            Self {
                enabled: Default::default(),
            }
        }
    }
    #[doc = "`AgentMessageDeltaNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"AgentMessageDeltaNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"delta\","]
    #[doc = "    \"itemId\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"delta\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"itemId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct AgentMessageDeltaNotification {
        pub delta: ::std::string::String,
        #[serde(rename = "itemId")]
        pub item_id: ::std::string::String,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`AgentPath`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    #[serde(transparent)]
    pub struct AgentPath(pub ::std::string::String);
    impl ::std::ops::Deref for AgentPath {
        type Target = ::std::string::String;
        fn deref(&self) -> &::std::string::String {
            &self.0
        }
    }
    impl ::std::convert::From<AgentPath> for ::std::string::String {
        fn from(value: AgentPath) -> Self {
            value.0
        }
    }
    impl ::std::convert::From<::std::string::String> for AgentPath {
        fn from(value: ::std::string::String) -> Self {
            Self(value)
        }
    }
    impl ::std::str::FromStr for AgentPath {
        type Err = ::std::convert::Infallible;
        fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
            Ok(Self(value.to_string()))
        }
    }
    impl ::std::fmt::Display for AgentPath {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            self.0.fmt(f)
        }
    }
    #[doc = "EXPERIMENTAL - app metadata returned by app-list APIs."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"EXPERIMENTAL - app metadata returned by app-list APIs.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"isDiscoverableApp\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"category\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"developer\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"isDiscoverableApp\": {"]
    #[doc = "      \"type\": \"boolean\""]
    #[doc = "    },"]
    #[doc = "    \"privacyPolicy\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"termsOfService\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"website\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct AppBranding {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub category: ::std::option::Option<::std::string::String>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub developer: ::std::option::Option<::std::string::String>,
        #[serde(rename = "isDiscoverableApp")]
        pub is_discoverable_app: bool,
        #[serde(
            rename = "privacyPolicy",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub privacy_policy: ::std::option::Option<::std::string::String>,
        #[serde(
            rename = "termsOfService",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub terms_of_service: ::std::option::Option<::std::string::String>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub website: ::std::option::Option<::std::string::String>,
    }
    #[doc = "EXPERIMENTAL - app metadata returned by app-list APIs."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"EXPERIMENTAL - app metadata returned by app-list APIs.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"id\","]
    #[doc = "    \"name\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"appMetadata\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/AppMetadata\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"branding\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/AppBranding\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"description\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"distributionChannel\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"iconAssets\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"object\","]
    #[doc = "          \"additionalProperties\": {"]
    #[doc = "            \"type\": \"string\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"iconDarkAssets\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"object\","]
    #[doc = "          \"additionalProperties\": {"]
    #[doc = "            \"type\": \"string\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"id\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"installUrl\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"isAccessible\": {"]
    #[doc = "      \"default\": false,"]
    #[doc = "      \"type\": \"boolean\""]
    #[doc = "    },"]
    #[doc = "    \"isEnabled\": {"]
    #[doc = "      \"description\": \"Whether this app is enabled in config.toml. Example: ```toml [apps.bad_app] enabled = false ```\","]
    #[doc = "      \"default\": true,"]
    #[doc = "      \"type\": \"boolean\""]
    #[doc = "    },"]
    #[doc = "    \"labels\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"object\","]
    #[doc = "          \"additionalProperties\": {"]
    #[doc = "            \"type\": \"string\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"logoUrl\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"logoUrlDark\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"name\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"pluginDisplayNames\": {"]
    #[doc = "      \"default\": [],"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"type\": \"string\""]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct AppInfo {
        #[serde(
            rename = "appMetadata",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub app_metadata: ::std::option::Option<AppMetadata>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub branding: ::std::option::Option<AppBranding>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub description: ::std::option::Option<::std::string::String>,
        #[serde(
            rename = "distributionChannel",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub distribution_channel: ::std::option::Option<::std::string::String>,
        #[serde(
            rename = "iconAssets",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub icon_assets: ::std::option::Option<
            ::std::collections::HashMap<::std::string::String, ::std::string::String>,
        >,
        #[serde(
            rename = "iconDarkAssets",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub icon_dark_assets: ::std::option::Option<
            ::std::collections::HashMap<::std::string::String, ::std::string::String>,
        >,
        pub id: ::std::string::String,
        #[serde(
            rename = "installUrl",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub install_url: ::std::option::Option<::std::string::String>,
        #[serde(rename = "isAccessible", default)]
        pub is_accessible: bool,
        #[doc = "Whether this app is enabled in config.toml. Example: ```toml [apps.bad_app] enabled = false ```"]
        #[serde(rename = "isEnabled", default = "defaults::default_bool::<true>")]
        pub is_enabled: bool,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub labels: ::std::option::Option<
            ::std::collections::HashMap<::std::string::String, ::std::string::String>,
        >,
        #[serde(
            rename = "logoUrl",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub logo_url: ::std::option::Option<::std::string::String>,
        #[serde(
            rename = "logoUrlDark",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub logo_url_dark: ::std::option::Option<::std::string::String>,
        pub name: ::std::string::String,
        #[serde(rename = "pluginDisplayNames", default)]
        pub plugin_display_names: ::std::vec::Vec<::std::string::String>,
    }
    #[doc = "EXPERIMENTAL - notification emitted when the app list changes."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"AppListUpdatedNotification\","]
    #[doc = "  \"description\": \"EXPERIMENTAL - notification emitted when the app list changes.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"data\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"data\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"$ref\": \"#/definitions/AppInfo\""]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct AppListUpdatedNotification {
        pub data: ::std::vec::Vec<AppInfo>,
    }
    #[doc = "`AppMetadata`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"categories\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"type\": \"string\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"developer\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"firstPartyRequiresInstall\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"boolean\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"firstPartyType\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"review\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/AppReview\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"screenshots\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/AppScreenshot\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"seoDescription\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"showInComposerWhenUnlinked\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"boolean\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"subCategories\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"type\": \"string\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"version\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"versionId\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"versionNotes\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct AppMetadata {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub categories: ::std::option::Option<::std::vec::Vec<::std::string::String>>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub developer: ::std::option::Option<::std::string::String>,
        #[serde(
            rename = "firstPartyRequiresInstall",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub first_party_requires_install: ::std::option::Option<bool>,
        #[serde(
            rename = "firstPartyType",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub first_party_type: ::std::option::Option<::std::string::String>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub review: ::std::option::Option<AppReview>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub screenshots: ::std::option::Option<::std::vec::Vec<AppScreenshot>>,
        #[serde(
            rename = "seoDescription",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub seo_description: ::std::option::Option<::std::string::String>,
        #[serde(
            rename = "showInComposerWhenUnlinked",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub show_in_composer_when_unlinked: ::std::option::Option<bool>,
        #[serde(
            rename = "subCategories",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub sub_categories: ::std::option::Option<::std::vec::Vec<::std::string::String>>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub version: ::std::option::Option<::std::string::String>,
        #[serde(
            rename = "versionId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub version_id: ::std::option::Option<::std::string::String>,
        #[serde(
            rename = "versionNotes",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub version_notes: ::std::option::Option<::std::string::String>,
    }
    impl ::std::default::Default for AppMetadata {
        fn default() -> Self {
            Self {
                categories: Default::default(),
                developer: Default::default(),
                first_party_requires_install: Default::default(),
                first_party_type: Default::default(),
                review: Default::default(),
                screenshots: Default::default(),
                seo_description: Default::default(),
                show_in_composer_when_unlinked: Default::default(),
                sub_categories: Default::default(),
                version: Default::default(),
                version_id: Default::default(),
                version_notes: Default::default(),
            }
        }
    }
    #[doc = "`AppReview`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"status\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"status\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct AppReview {
        pub status: ::std::string::String,
    }
    #[doc = "`AppScreenshot`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"userPrompt\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"fileId\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"url\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"userPrompt\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct AppScreenshot {
        #[serde(
            rename = "fileId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub file_id: ::std::option::Option<::std::string::String>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub url: ::std::option::Option<::std::string::String>,
        #[serde(rename = "userPrompt")]
        pub user_prompt: ::std::string::String,
    }
    #[doc = "Configures who approval requests are routed to for review. Examples include sandbox escapes, blocked network access, MCP approval prompts, and ARC escalations. Defaults to `user`. `auto_review` uses a carefully prompted subagent to gather relevant context and apply a risk-based decision framework before approving or denying the request. The legacy value `guardian_subagent` is accepted for compatibility."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"Configures who approval requests are routed to for review. Examples include sandbox escapes, blocked network access, MCP approval prompts, and ARC escalations. Defaults to `user`. `auto_review` uses a carefully prompted subagent to gather relevant context and apply a risk-based decision framework before approving or denying the request. The legacy value `guardian_subagent` is accepted for compatibility.\","]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"user\","]
    #[doc = "    \"auto_review\","]
    #[doc = "    \"guardian_subagent\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum ApprovalsReviewer {
        #[serde(rename = "user")]
        User,
        #[serde(rename = "auto_review")]
        AutoReview,
        #[serde(rename = "guardian_subagent")]
        GuardianSubagent,
    }
    impl ::std::fmt::Display for ApprovalsReviewer {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::User => f.write_str("user"),
                Self::AutoReview => f.write_str("auto_review"),
                Self::GuardianSubagent => f.write_str("guardian_subagent"),
            }
        }
    }
    impl ::std::str::FromStr for ApprovalsReviewer {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "user" => Ok(Self::User),
                "auto_review" => Ok(Self::AutoReview),
                "guardian_subagent" => Ok(Self::GuardianSubagent),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for ApprovalsReviewer {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for ApprovalsReviewer {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for ApprovalsReviewer {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`AskForApproval`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"untrusted\","]
    #[doc = "        \"on-request\","]
    #[doc = "        \"never\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"GranularAskForApproval\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"granular\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"granular\": {"]
    #[doc = "          \"type\": \"object\","]
    #[doc = "          \"required\": ["]
    #[doc = "            \"mcp_elicitations\","]
    #[doc = "            \"rules\","]
    #[doc = "            \"sandbox_approval\""]
    #[doc = "          ],"]
    #[doc = "          \"properties\": {"]
    #[doc = "            \"mcp_elicitations\": {"]
    #[doc = "              \"type\": \"boolean\""]
    #[doc = "            },"]
    #[doc = "            \"request_permissions\": {"]
    #[doc = "              \"default\": false,"]
    #[doc = "              \"type\": \"boolean\""]
    #[doc = "            },"]
    #[doc = "            \"rules\": {"]
    #[doc = "              \"type\": \"boolean\""]
    #[doc = "            },"]
    #[doc = "            \"sandbox_approval\": {"]
    #[doc = "              \"type\": \"boolean\""]
    #[doc = "            },"]
    #[doc = "            \"skill_approval\": {"]
    #[doc = "              \"default\": false,"]
    #[doc = "              \"type\": \"boolean\""]
    #[doc = "            }"]
    #[doc = "          }"]
    #[doc = "        }"]
    #[doc = "      },"]
    #[doc = "      \"additionalProperties\": false"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub enum AskForApproval {
        #[serde(rename = "untrusted")]
        Untrusted,
        #[serde(rename = "on-request")]
        OnRequest,
        #[serde(rename = "never")]
        Never,
        #[serde(rename = "granular")]
        Granular {
            mcp_elicitations: bool,
            #[serde(default)]
            request_permissions: bool,
            rules: bool,
            sandbox_approval: bool,
            #[serde(default)]
            skill_approval: bool,
        },
    }
    #[doc = "Authentication mode for OpenAI-backed providers."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"Authentication mode for OpenAI-backed providers.\","]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"description\": \"OpenAI API key provided by the caller and stored by Codex.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"apikey\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"description\": \"ChatGPT OAuth managed by Codex (tokens persisted and refreshed by Codex).\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"chatgpt\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"description\": \"[UNSTABLE] FOR OPENAI INTERNAL USE ONLY - DO NOT USE.\\n\\nChatGPT auth tokens are supplied by an external host app and are only stored in memory. Token refresh must be handled by the external host app.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"chatgptAuthTokens\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"description\": \"Backend auth supplied as request headers.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"headers\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"description\": \"Programmatic Codex auth backed by a registered Agent Identity.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"agentIdentity\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"description\": \"Programmatic Codex auth backed by a personal access token.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"personalAccessToken\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"description\": \"Amazon Bedrock bearer token managed by Codex.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"bedrockApiKey\""]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum AuthMode {
        #[doc = "OpenAI API key provided by the caller and stored by Codex."]
        #[serde(rename = "apikey")]
        Apikey,
        #[doc = "ChatGPT OAuth managed by Codex (tokens persisted and refreshed by Codex)."]
        #[serde(rename = "chatgpt")]
        Chatgpt,
        #[doc = "[UNSTABLE] FOR OPENAI INTERNAL USE ONLY - DO NOT USE.\n\nChatGPT auth tokens are supplied by an external host app and are only stored in memory. Token refresh must be handled by the external host app."]
        #[serde(rename = "chatgptAuthTokens")]
        ChatgptAuthTokens,
        #[doc = "Backend auth supplied as request headers."]
        #[serde(rename = "headers")]
        Headers,
        #[doc = "Programmatic Codex auth backed by a registered Agent Identity."]
        #[serde(rename = "agentIdentity")]
        AgentIdentity,
        #[doc = "Programmatic Codex auth backed by a personal access token."]
        #[serde(rename = "personalAccessToken")]
        PersonalAccessToken,
        #[doc = "Amazon Bedrock bearer token managed by Codex."]
        #[serde(rename = "bedrockApiKey")]
        BedrockApiKey,
    }
    impl ::std::fmt::Display for AuthMode {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Apikey => f.write_str("apikey"),
                Self::Chatgpt => f.write_str("chatgpt"),
                Self::ChatgptAuthTokens => f.write_str("chatgptAuthTokens"),
                Self::Headers => f.write_str("headers"),
                Self::AgentIdentity => f.write_str("agentIdentity"),
                Self::PersonalAccessToken => f.write_str("personalAccessToken"),
                Self::BedrockApiKey => f.write_str("bedrockApiKey"),
            }
        }
    }
    impl ::std::str::FromStr for AuthMode {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "apikey" => Ok(Self::Apikey),
                "chatgpt" => Ok(Self::Chatgpt),
                "chatgptAuthTokens" => Ok(Self::ChatgptAuthTokens),
                "headers" => Ok(Self::Headers),
                "agentIdentity" => Ok(Self::AgentIdentity),
                "personalAccessToken" => Ok(Self::PersonalAccessToken),
                "bedrockApiKey" => Ok(Self::BedrockApiKey),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for AuthMode {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for AuthMode {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for AuthMode {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "[UNSTABLE] Source that produced a terminal approval auto-review decision."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"[UNSTABLE] Source that produced a terminal approval auto-review decision.\","]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"agent\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum AutoReviewDecisionSource {
        #[serde(rename = "agent")]
        Agent,
    }
    impl ::std::fmt::Display for AutoReviewDecisionSource {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Agent => f.write_str("agent"),
            }
        }
    }
    impl ::std::str::FromStr for AutoReviewDecisionSource {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "agent" => Ok(Self::Agent),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for AutoReviewDecisionSource {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for AutoReviewDecisionSource {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for AutoReviewDecisionSource {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`ByteRange`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"end\","]
    #[doc = "    \"start\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"end\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"uint\","]
    #[doc = "      \"minimum\": 0.0"]
    #[doc = "    },"]
    #[doc = "    \"start\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"uint\","]
    #[doc = "      \"minimum\": 0.0"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ByteRange {
        pub end: u32,
        pub start: u32,
    }
    #[doc = "`CodexConversationRoot`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"CodexConversationRoot\","]
    #[doc = "  \"$ref\": \"#/definitions/ServerNotification\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(transparent)]
    pub struct CodexConversationRoot(pub ServerNotification);
    impl ::std::ops::Deref for CodexConversationRoot {
        type Target = ServerNotification;
        fn deref(&self) -> &ServerNotification {
            &self.0
        }
    }
    impl ::std::convert::From<CodexConversationRoot> for ServerNotification {
        fn from(value: CodexConversationRoot) -> Self {
            value.0
        }
    }
    impl ::std::convert::From<ServerNotification> for CodexConversationRoot {
        fn from(value: ServerNotification) -> Self {
            Self(value)
        }
    }
    #[doc = "This translation layer make sure that we expose codex error code in camel case.\n\nWhen an upstream HTTP status is available (for example, from the Responses API or a provider), it is forwarded in `httpStatusCode` on the relevant `codexErrorInfo` variant."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"This translation layer make sure that we expose codex error code in camel case.\\n\\nWhen an upstream HTTP status is available (for example, from the Responses API or a provider), it is forwarded in `httpStatusCode` on the relevant `codexErrorInfo` variant.\","]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"contextWindowExceeded\","]
    #[doc = "        \"sessionBudgetExceeded\","]
    #[doc = "        \"usageLimitExceeded\","]
    #[doc = "        \"serverOverloaded\","]
    #[doc = "        \"cyberPolicy\","]
    #[doc = "        \"internalServerError\","]
    #[doc = "        \"unauthorized\","]
    #[doc = "        \"badRequest\","]
    #[doc = "        \"threadRollbackFailed\","]
    #[doc = "        \"sandboxError\","]
    #[doc = "        \"other\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"HttpConnectionFailedCodexErrorInfo\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"httpConnectionFailed\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"httpConnectionFailed\": {"]
    #[doc = "          \"type\": \"object\","]
    #[doc = "          \"properties\": {"]
    #[doc = "            \"httpStatusCode\": {"]
    #[doc = "              \"oneOf\": ["]
    #[doc = "                {"]
    #[doc = "                  \"type\": \"integer\","]
    #[doc = "                  \"format\": \"uint16\","]
    #[doc = "                  \"minimum\": 0.0"]
    #[doc = "                },"]
    #[doc = "                {"]
    #[doc = "                  \"type\": \"null\""]
    #[doc = "                }"]
    #[doc = "              ]"]
    #[doc = "            }"]
    #[doc = "          }"]
    #[doc = "        }"]
    #[doc = "      },"]
    #[doc = "      \"additionalProperties\": false"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ResponseStreamConnectionFailedCodexErrorInfo\","]
    #[doc = "      \"description\": \"Failed to connect to the response SSE stream.\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"responseStreamConnectionFailed\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"responseStreamConnectionFailed\": {"]
    #[doc = "          \"type\": \"object\","]
    #[doc = "          \"properties\": {"]
    #[doc = "            \"httpStatusCode\": {"]
    #[doc = "              \"oneOf\": ["]
    #[doc = "                {"]
    #[doc = "                  \"type\": \"integer\","]
    #[doc = "                  \"format\": \"uint16\","]
    #[doc = "                  \"minimum\": 0.0"]
    #[doc = "                },"]
    #[doc = "                {"]
    #[doc = "                  \"type\": \"null\""]
    #[doc = "                }"]
    #[doc = "              ]"]
    #[doc = "            }"]
    #[doc = "          }"]
    #[doc = "        }"]
    #[doc = "      },"]
    #[doc = "      \"additionalProperties\": false"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ResponseStreamDisconnectedCodexErrorInfo\","]
    #[doc = "      \"description\": \"The response SSE stream disconnected in the middle of a turn before completion.\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"responseStreamDisconnected\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"responseStreamDisconnected\": {"]
    #[doc = "          \"type\": \"object\","]
    #[doc = "          \"properties\": {"]
    #[doc = "            \"httpStatusCode\": {"]
    #[doc = "              \"oneOf\": ["]
    #[doc = "                {"]
    #[doc = "                  \"type\": \"integer\","]
    #[doc = "                  \"format\": \"uint16\","]
    #[doc = "                  \"minimum\": 0.0"]
    #[doc = "                },"]
    #[doc = "                {"]
    #[doc = "                  \"type\": \"null\""]
    #[doc = "                }"]
    #[doc = "              ]"]
    #[doc = "            }"]
    #[doc = "          }"]
    #[doc = "        }"]
    #[doc = "      },"]
    #[doc = "      \"additionalProperties\": false"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ResponseTooManyFailedAttemptsCodexErrorInfo\","]
    #[doc = "      \"description\": \"Reached the retry limit for responses.\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"responseTooManyFailedAttempts\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"responseTooManyFailedAttempts\": {"]
    #[doc = "          \"type\": \"object\","]
    #[doc = "          \"properties\": {"]
    #[doc = "            \"httpStatusCode\": {"]
    #[doc = "              \"oneOf\": ["]
    #[doc = "                {"]
    #[doc = "                  \"type\": \"integer\","]
    #[doc = "                  \"format\": \"uint16\","]
    #[doc = "                  \"minimum\": 0.0"]
    #[doc = "                },"]
    #[doc = "                {"]
    #[doc = "                  \"type\": \"null\""]
    #[doc = "                }"]
    #[doc = "              ]"]
    #[doc = "            }"]
    #[doc = "          }"]
    #[doc = "        }"]
    #[doc = "      },"]
    #[doc = "      \"additionalProperties\": false"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ActiveTurnNotSteerableCodexErrorInfo\","]
    #[doc = "      \"description\": \"Returned when `turn/start` or `turn/steer` is submitted while the current active turn cannot accept same-turn steering, for example `/review` or manual `/compact`.\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"activeTurnNotSteerable\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"activeTurnNotSteerable\": {"]
    #[doc = "          \"type\": \"object\","]
    #[doc = "          \"required\": ["]
    #[doc = "            \"turnKind\""]
    #[doc = "          ],"]
    #[doc = "          \"properties\": {"]
    #[doc = "            \"turnKind\": {"]
    #[doc = "              \"$ref\": \"#/definitions/NonSteerableTurnKind\""]
    #[doc = "            }"]
    #[doc = "          }"]
    #[doc = "        }"]
    #[doc = "      },"]
    #[doc = "      \"additionalProperties\": false"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub enum CodexErrorInfo {
        #[serde(rename = "contextWindowExceeded")]
        ContextWindowExceeded,
        #[serde(rename = "sessionBudgetExceeded")]
        SessionBudgetExceeded,
        #[serde(rename = "usageLimitExceeded")]
        UsageLimitExceeded,
        #[serde(rename = "serverOverloaded")]
        ServerOverloaded,
        #[serde(rename = "cyberPolicy")]
        CyberPolicy,
        #[serde(rename = "internalServerError")]
        InternalServerError,
        #[serde(rename = "unauthorized")]
        Unauthorized,
        #[serde(rename = "badRequest")]
        BadRequest,
        #[serde(rename = "threadRollbackFailed")]
        ThreadRollbackFailed,
        #[serde(rename = "sandboxError")]
        SandboxError,
        #[serde(rename = "other")]
        Other,
        #[serde(rename = "httpConnectionFailed")]
        HttpConnectionFailed {
            #[serde(
                rename = "httpStatusCode",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            http_status_code: ::std::option::Option<::std::option::Option<u16>>,
        },
        #[doc = "Failed to connect to the response SSE stream."]
        #[serde(rename = "responseStreamConnectionFailed")]
        ResponseStreamConnectionFailed {
            #[serde(
                rename = "httpStatusCode",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            http_status_code: ::std::option::Option<::std::option::Option<u16>>,
        },
        #[doc = "The response SSE stream disconnected in the middle of a turn before completion."]
        #[serde(rename = "responseStreamDisconnected")]
        ResponseStreamDisconnected {
            #[serde(
                rename = "httpStatusCode",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            http_status_code: ::std::option::Option<::std::option::Option<u16>>,
        },
        #[doc = "Reached the retry limit for responses."]
        #[serde(rename = "responseTooManyFailedAttempts")]
        ResponseTooManyFailedAttempts {
            #[serde(
                rename = "httpStatusCode",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            http_status_code: ::std::option::Option<::std::option::Option<u16>>,
        },
        #[doc = "Returned when `turn/start` or `turn/steer` is submitted while the current active turn cannot accept same-turn steering, for example `/review` or manual `/compact`."]
        #[serde(rename = "activeTurnNotSteerable")]
        ActiveTurnNotSteerable {
            #[serde(rename = "turnKind")]
            turn_kind: NonSteerableTurnKind,
        },
    }
    #[doc = "`CollabAgentState`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"status\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"message\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"status\": {"]
    #[doc = "      \"$ref\": \"#/definitions/CollabAgentStatus\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct CollabAgentState {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub message: ::std::option::Option<::std::string::String>,
        pub status: CollabAgentStatus,
    }
    #[doc = "`CollabAgentStatus`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"pendingInit\","]
    #[doc = "    \"running\","]
    #[doc = "    \"interrupted\","]
    #[doc = "    \"completed\","]
    #[doc = "    \"errored\","]
    #[doc = "    \"shutdown\","]
    #[doc = "    \"notFound\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum CollabAgentStatus {
        #[serde(rename = "pendingInit")]
        PendingInit,
        #[serde(rename = "running")]
        Running,
        #[serde(rename = "interrupted")]
        Interrupted,
        #[serde(rename = "completed")]
        Completed,
        #[serde(rename = "errored")]
        Errored,
        #[serde(rename = "shutdown")]
        Shutdown,
        #[serde(rename = "notFound")]
        NotFound,
    }
    impl ::std::fmt::Display for CollabAgentStatus {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::PendingInit => f.write_str("pendingInit"),
                Self::Running => f.write_str("running"),
                Self::Interrupted => f.write_str("interrupted"),
                Self::Completed => f.write_str("completed"),
                Self::Errored => f.write_str("errored"),
                Self::Shutdown => f.write_str("shutdown"),
                Self::NotFound => f.write_str("notFound"),
            }
        }
    }
    impl ::std::str::FromStr for CollabAgentStatus {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "pendingInit" => Ok(Self::PendingInit),
                "running" => Ok(Self::Running),
                "interrupted" => Ok(Self::Interrupted),
                "completed" => Ok(Self::Completed),
                "errored" => Ok(Self::Errored),
                "shutdown" => Ok(Self::Shutdown),
                "notFound" => Ok(Self::NotFound),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for CollabAgentStatus {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for CollabAgentStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for CollabAgentStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`CollabAgentTool`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"spawnAgent\","]
    #[doc = "    \"sendInput\","]
    #[doc = "    \"resumeAgent\","]
    #[doc = "    \"wait\","]
    #[doc = "    \"closeAgent\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum CollabAgentTool {
        #[serde(rename = "spawnAgent")]
        SpawnAgent,
        #[serde(rename = "sendInput")]
        SendInput,
        #[serde(rename = "resumeAgent")]
        ResumeAgent,
        #[serde(rename = "wait")]
        Wait,
        #[serde(rename = "closeAgent")]
        CloseAgent,
    }
    impl ::std::fmt::Display for CollabAgentTool {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::SpawnAgent => f.write_str("spawnAgent"),
                Self::SendInput => f.write_str("sendInput"),
                Self::ResumeAgent => f.write_str("resumeAgent"),
                Self::Wait => f.write_str("wait"),
                Self::CloseAgent => f.write_str("closeAgent"),
            }
        }
    }
    impl ::std::str::FromStr for CollabAgentTool {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "spawnAgent" => Ok(Self::SpawnAgent),
                "sendInput" => Ok(Self::SendInput),
                "resumeAgent" => Ok(Self::ResumeAgent),
                "wait" => Ok(Self::Wait),
                "closeAgent" => Ok(Self::CloseAgent),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for CollabAgentTool {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for CollabAgentTool {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for CollabAgentTool {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`CollabAgentToolCallStatus`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"inProgress\","]
    #[doc = "    \"completed\","]
    #[doc = "    \"failed\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum CollabAgentToolCallStatus {
        #[serde(rename = "inProgress")]
        InProgress,
        #[serde(rename = "completed")]
        Completed,
        #[serde(rename = "failed")]
        Failed,
    }
    impl ::std::fmt::Display for CollabAgentToolCallStatus {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::InProgress => f.write_str("inProgress"),
                Self::Completed => f.write_str("completed"),
                Self::Failed => f.write_str("failed"),
            }
        }
    }
    impl ::std::str::FromStr for CollabAgentToolCallStatus {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "inProgress" => Ok(Self::InProgress),
                "completed" => Ok(Self::Completed),
                "failed" => Ok(Self::Failed),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for CollabAgentToolCallStatus {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for CollabAgentToolCallStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for CollabAgentToolCallStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "Collaboration mode for a Codex session."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"Collaboration mode for a Codex session.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"mode\","]
    #[doc = "    \"settings\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"mode\": {"]
    #[doc = "      \"$ref\": \"#/definitions/ModeKind\""]
    #[doc = "    },"]
    #[doc = "    \"settings\": {"]
    #[doc = "      \"$ref\": \"#/definitions/Settings\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct CollaborationMode {
        pub mode: ModeKind,
        pub settings: Settings,
    }
    #[doc = "`CommandAction`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ReadCommandAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"command\","]
    #[doc = "        \"name\","]
    #[doc = "        \"path\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"command\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"name\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"$ref\": \"#/definitions/AbsolutePathBuf\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ReadCommandActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"read\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ListFilesCommandAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"command\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"command\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ListFilesCommandActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"listFiles\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SearchCommandAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"command\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"command\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"query\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"SearchCommandActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"search\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"UnknownCommandAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"command\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"command\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"UnknownCommandActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"unknown\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type")]
    pub enum CommandAction {
        #[doc = "ReadCommandAction"]
        #[serde(rename = "read")]
        Read {
            command: ::std::string::String,
            name: ::std::string::String,
            path: AbsolutePathBuf,
        },
        #[doc = "ListFilesCommandAction"]
        #[serde(rename = "listFiles")]
        ListFiles {
            command: ::std::string::String,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            path: ::std::option::Option<::std::option::Option<::std::string::String>>,
        },
        #[doc = "SearchCommandAction"]
        #[serde(rename = "search")]
        Search {
            command: ::std::string::String,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            path: ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            query: ::std::option::Option<::std::option::Option<::std::string::String>>,
        },
        #[doc = "UnknownCommandAction"]
        #[serde(rename = "unknown")]
        Unknown { command: ::std::string::String },
    }
    #[doc = "Base64-encoded output chunk emitted for a streaming `command/exec` request.\n\nThese notifications are connection-scoped. If the originating connection closes, the server terminates the process."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"CommandExecOutputDeltaNotification\","]
    #[doc = "  \"description\": \"Base64-encoded output chunk emitted for a streaming `command/exec` request.\\n\\nThese notifications are connection-scoped. If the originating connection closes, the server terminates the process.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"capReached\","]
    #[doc = "    \"deltaBase64\","]
    #[doc = "    \"processId\","]
    #[doc = "    \"stream\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"capReached\": {"]
    #[doc = "      \"description\": \"`true` on the final streamed chunk for a stream when `outputBytesCap` truncated later output on that stream.\","]
    #[doc = "      \"type\": \"boolean\""]
    #[doc = "    },"]
    #[doc = "    \"deltaBase64\": {"]
    #[doc = "      \"description\": \"Base64-encoded output bytes.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"processId\": {"]
    #[doc = "      \"description\": \"Client-supplied, connection-scoped `processId` from the original `command/exec` request.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"stream\": {"]
    #[doc = "      \"description\": \"Output stream for this chunk.\","]
    #[doc = "      \"allOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/CommandExecOutputStream\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct CommandExecOutputDeltaNotification {
        #[doc = "`true` on the final streamed chunk for a stream when `outputBytesCap` truncated later output on that stream."]
        #[serde(rename = "capReached")]
        pub cap_reached: bool,
        #[doc = "Base64-encoded output bytes."]
        #[serde(rename = "deltaBase64")]
        pub delta_base64: ::std::string::String,
        #[doc = "Client-supplied, connection-scoped `processId` from the original `command/exec` request."]
        #[serde(rename = "processId")]
        pub process_id: ::std::string::String,
        #[doc = "Output stream for this chunk."]
        pub stream: CommandExecOutputStream,
    }
    #[doc = "Stream label for `command/exec/outputDelta` notifications."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"Stream label for `command/exec/outputDelta` notifications.\","]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"description\": \"stdout stream. PTY mode multiplexes terminal output here.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"stdout\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"description\": \"stderr stream.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"stderr\""]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum CommandExecOutputStream {
        #[doc = "stdout stream. PTY mode multiplexes terminal output here."]
        #[serde(rename = "stdout")]
        Stdout,
        #[doc = "stderr stream."]
        #[serde(rename = "stderr")]
        Stderr,
    }
    impl ::std::fmt::Display for CommandExecOutputStream {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Stdout => f.write_str("stdout"),
                Self::Stderr => f.write_str("stderr"),
            }
        }
    }
    impl ::std::str::FromStr for CommandExecOutputStream {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "stdout" => Ok(Self::Stdout),
                "stderr" => Ok(Self::Stderr),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for CommandExecOutputStream {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for CommandExecOutputStream {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for CommandExecOutputStream {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`CommandExecutionOutputDeltaNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"CommandExecutionOutputDeltaNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"delta\","]
    #[doc = "    \"itemId\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"delta\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"itemId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct CommandExecutionOutputDeltaNotification {
        pub delta: ::std::string::String,
        #[serde(rename = "itemId")]
        pub item_id: ::std::string::String,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`CommandExecutionSource`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"agent\","]
    #[doc = "    \"userShell\","]
    #[doc = "    \"unifiedExecStartup\","]
    #[doc = "    \"unifiedExecInteraction\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum CommandExecutionSource {
        #[serde(rename = "agent")]
        Agent,
        #[serde(rename = "userShell")]
        UserShell,
        #[serde(rename = "unifiedExecStartup")]
        UnifiedExecStartup,
        #[serde(rename = "unifiedExecInteraction")]
        UnifiedExecInteraction,
    }
    impl ::std::fmt::Display for CommandExecutionSource {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Agent => f.write_str("agent"),
                Self::UserShell => f.write_str("userShell"),
                Self::UnifiedExecStartup => f.write_str("unifiedExecStartup"),
                Self::UnifiedExecInteraction => f.write_str("unifiedExecInteraction"),
            }
        }
    }
    impl ::std::str::FromStr for CommandExecutionSource {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "agent" => Ok(Self::Agent),
                "userShell" => Ok(Self::UserShell),
                "unifiedExecStartup" => Ok(Self::UnifiedExecStartup),
                "unifiedExecInteraction" => Ok(Self::UnifiedExecInteraction),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for CommandExecutionSource {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for CommandExecutionSource {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for CommandExecutionSource {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`CommandExecutionStatus`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"inProgress\","]
    #[doc = "    \"completed\","]
    #[doc = "    \"failed\","]
    #[doc = "    \"declined\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum CommandExecutionStatus {
        #[serde(rename = "inProgress")]
        InProgress,
        #[serde(rename = "completed")]
        Completed,
        #[serde(rename = "failed")]
        Failed,
        #[serde(rename = "declined")]
        Declined,
    }
    impl ::std::fmt::Display for CommandExecutionStatus {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::InProgress => f.write_str("inProgress"),
                Self::Completed => f.write_str("completed"),
                Self::Failed => f.write_str("failed"),
                Self::Declined => f.write_str("declined"),
            }
        }
    }
    impl ::std::str::FromStr for CommandExecutionStatus {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "inProgress" => Ok(Self::InProgress),
                "completed" => Ok(Self::Completed),
                "failed" => Ok(Self::Failed),
                "declined" => Ok(Self::Declined),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for CommandExecutionStatus {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for CommandExecutionStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for CommandExecutionStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`ConfigWarningNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ConfigWarningNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"summary\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"details\": {"]
    #[doc = "      \"description\": \"Optional extra guidance or error details.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Optional extra guidance or error details.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"path\": {"]
    #[doc = "      \"description\": \"Optional path to the config file that triggered the warning.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Optional path to the config file that triggered the warning.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"range\": {"]
    #[doc = "      \"description\": \"Optional range for the error location inside the config file.\","]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/TextRange\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"summary\": {"]
    #[doc = "      \"description\": \"Concise summary of the warning.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ConfigWarningNotification {
        #[doc = "Optional extra guidance or error details."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub details: ::std::option::Option<::std::option::Option<::std::string::String>>,
        #[doc = "Optional path to the config file that triggered the warning."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub path: ::std::option::Option<::std::option::Option<::std::string::String>>,
        #[doc = "Optional range for the error location inside the config file."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub range: ::std::option::Option<::std::option::Option<TextRange>>,
        #[doc = "Concise summary of the warning."]
        pub summary: ::std::string::String,
    }
    #[doc = "Deprecated: Use `ContextCompaction` item type instead."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ContextCompactedNotification\","]
    #[doc = "  \"description\": \"Deprecated: Use `ContextCompaction` item type instead.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ContextCompactedNotification {
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`CreditsSnapshot`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"hasCredits\","]
    #[doc = "    \"unlimited\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"balance\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"hasCredits\": {"]
    #[doc = "      \"type\": \"boolean\""]
    #[doc = "    },"]
    #[doc = "    \"unlimited\": {"]
    #[doc = "      \"type\": \"boolean\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct CreditsSnapshot {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub balance: ::std::option::Option<::std::string::String>,
        #[serde(rename = "hasCredits")]
        pub has_credits: bool,
        pub unlimited: bool,
    }
    #[doc = "`DeprecationNoticeNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"DeprecationNoticeNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"summary\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"details\": {"]
    #[doc = "      \"description\": \"Optional extra guidance, such as migration steps or rationale.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Optional extra guidance, such as migration steps or rationale.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"summary\": {"]
    #[doc = "      \"description\": \"Concise summary of what is deprecated.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct DeprecationNoticeNotification {
        #[doc = "Optional extra guidance, such as migration steps or rationale."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub details: ::std::option::Option<::std::option::Option<::std::string::String>>,
        #[doc = "Concise summary of what is deprecated."]
        pub summary: ::std::string::String,
    }
    #[doc = "`DynamicToolCallOutputContentItem`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"InputTextDynamicToolCallOutputContentItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"text\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"text\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"InputTextDynamicToolCallOutputContentItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"inputText\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"InputImageDynamicToolCallOutputContentItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"imageUrl\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"imageUrl\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"InputImageDynamicToolCallOutputContentItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"inputImage\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type")]
    pub enum DynamicToolCallOutputContentItem {
        #[doc = "InputTextDynamicToolCallOutputContentItem"]
        #[serde(rename = "inputText")]
        InputText { text: ::std::string::String },
        #[doc = "InputImageDynamicToolCallOutputContentItem"]
        #[serde(rename = "inputImage")]
        InputImage {
            #[serde(rename = "imageUrl")]
            image_url: ::std::string::String,
        },
    }
    #[doc = "`DynamicToolCallStatus`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"inProgress\","]
    #[doc = "    \"completed\","]
    #[doc = "    \"failed\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum DynamicToolCallStatus {
        #[serde(rename = "inProgress")]
        InProgress,
        #[serde(rename = "completed")]
        Completed,
        #[serde(rename = "failed")]
        Failed,
    }
    impl ::std::fmt::Display for DynamicToolCallStatus {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::InProgress => f.write_str("inProgress"),
                Self::Completed => f.write_str("completed"),
                Self::Failed => f.write_str("failed"),
            }
        }
    }
    impl ::std::str::FromStr for DynamicToolCallStatus {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "inProgress" => Ok(Self::InProgress),
                "completed" => Ok(Self::Completed),
                "failed" => Ok(Self::Failed),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for DynamicToolCallStatus {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for DynamicToolCallStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for DynamicToolCallStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`ErrorNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ErrorNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"error\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\","]
    #[doc = "    \"willRetry\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"error\": {"]
    #[doc = "      \"$ref\": \"#/definitions/TurnError\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"willRetry\": {"]
    #[doc = "      \"type\": \"boolean\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ErrorNotification {
        pub error: TurnError,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
        #[serde(rename = "willRetry")]
        pub will_retry: bool,
    }
    #[doc = "`ExternalAgentConfigImportCompletedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ExternalAgentConfigImportCompletedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"importId\","]
    #[doc = "    \"itemTypeResults\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"importId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"itemTypeResults\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"$ref\": \"#/definitions/ExternalAgentConfigImportTypeResult\""]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ExternalAgentConfigImportCompletedNotification {
        #[serde(rename = "importId")]
        pub import_id: ::std::string::String,
        #[serde(rename = "itemTypeResults")]
        pub item_type_results: ::std::vec::Vec<ExternalAgentConfigImportTypeResult>,
    }
    #[doc = "`ExternalAgentConfigImportItemTypeFailure`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"failureStage\","]
    #[doc = "    \"itemType\","]
    #[doc = "    \"message\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"cwd\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"errorType\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"failureStage\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"itemType\": {"]
    #[doc = "      \"$ref\": \"#/definitions/ExternalAgentConfigMigrationItemType\""]
    #[doc = "    },"]
    #[doc = "    \"message\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"source\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ExternalAgentConfigImportItemTypeFailure {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub cwd: ::std::option::Option<::std::string::String>,
        #[serde(
            rename = "errorType",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub error_type: ::std::option::Option<::std::string::String>,
        #[serde(rename = "failureStage")]
        pub failure_stage: ::std::string::String,
        #[serde(rename = "itemType")]
        pub item_type: ExternalAgentConfigMigrationItemType,
        pub message: ::std::string::String,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub source: ::std::option::Option<::std::string::String>,
    }
    #[doc = "`ExternalAgentConfigImportItemTypeSuccess`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"itemType\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"cwd\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"itemType\": {"]
    #[doc = "      \"$ref\": \"#/definitions/ExternalAgentConfigMigrationItemType\""]
    #[doc = "    },"]
    #[doc = "    \"source\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"target\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ExternalAgentConfigImportItemTypeSuccess {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub cwd: ::std::option::Option<::std::string::String>,
        #[serde(rename = "itemType")]
        pub item_type: ExternalAgentConfigMigrationItemType,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub source: ::std::option::Option<::std::string::String>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub target: ::std::option::Option<::std::string::String>,
    }
    #[doc = "`ExternalAgentConfigImportProgressNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ExternalAgentConfigImportProgressNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"importId\","]
    #[doc = "    \"itemTypeResults\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"importId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"itemTypeResults\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"$ref\": \"#/definitions/ExternalAgentConfigImportTypeResult\""]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ExternalAgentConfigImportProgressNotification {
        #[serde(rename = "importId")]
        pub import_id: ::std::string::String,
        #[serde(rename = "itemTypeResults")]
        pub item_type_results: ::std::vec::Vec<ExternalAgentConfigImportTypeResult>,
    }
    #[doc = "`ExternalAgentConfigImportTypeResult`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"failures\","]
    #[doc = "    \"itemType\","]
    #[doc = "    \"successes\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"failures\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"$ref\": \"#/definitions/ExternalAgentConfigImportItemTypeFailure\""]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    \"itemType\": {"]
    #[doc = "      \"$ref\": \"#/definitions/ExternalAgentConfigMigrationItemType\""]
    #[doc = "    },"]
    #[doc = "    \"successes\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"$ref\": \"#/definitions/ExternalAgentConfigImportItemTypeSuccess\""]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ExternalAgentConfigImportTypeResult {
        pub failures: ::std::vec::Vec<ExternalAgentConfigImportItemTypeFailure>,
        #[serde(rename = "itemType")]
        pub item_type: ExternalAgentConfigMigrationItemType,
        pub successes: ::std::vec::Vec<ExternalAgentConfigImportItemTypeSuccess>,
    }
    #[doc = "`ExternalAgentConfigMigrationItemType`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"AGENTS_MD\","]
    #[doc = "    \"CONFIG\","]
    #[doc = "    \"SKILLS\","]
    #[doc = "    \"PLUGINS\","]
    #[doc = "    \"MCP_SERVER_CONFIG\","]
    #[doc = "    \"SUBAGENTS\","]
    #[doc = "    \"HOOKS\","]
    #[doc = "    \"COMMANDS\","]
    #[doc = "    \"SESSIONS\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum ExternalAgentConfigMigrationItemType {
        #[serde(rename = "AGENTS_MD")]
        AgentsMd,
        #[serde(rename = "CONFIG")]
        Config,
        #[serde(rename = "SKILLS")]
        Skills,
        #[serde(rename = "PLUGINS")]
        Plugins,
        #[serde(rename = "MCP_SERVER_CONFIG")]
        McpServerConfig,
        #[serde(rename = "SUBAGENTS")]
        Subagents,
        #[serde(rename = "HOOKS")]
        Hooks,
        #[serde(rename = "COMMANDS")]
        Commands,
        #[serde(rename = "SESSIONS")]
        Sessions,
    }
    impl ::std::fmt::Display for ExternalAgentConfigMigrationItemType {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::AgentsMd => f.write_str("AGENTS_MD"),
                Self::Config => f.write_str("CONFIG"),
                Self::Skills => f.write_str("SKILLS"),
                Self::Plugins => f.write_str("PLUGINS"),
                Self::McpServerConfig => f.write_str("MCP_SERVER_CONFIG"),
                Self::Subagents => f.write_str("SUBAGENTS"),
                Self::Hooks => f.write_str("HOOKS"),
                Self::Commands => f.write_str("COMMANDS"),
                Self::Sessions => f.write_str("SESSIONS"),
            }
        }
    }
    impl ::std::str::FromStr for ExternalAgentConfigMigrationItemType {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "AGENTS_MD" => Ok(Self::AgentsMd),
                "CONFIG" => Ok(Self::Config),
                "SKILLS" => Ok(Self::Skills),
                "PLUGINS" => Ok(Self::Plugins),
                "MCP_SERVER_CONFIG" => Ok(Self::McpServerConfig),
                "SUBAGENTS" => Ok(Self::Subagents),
                "HOOKS" => Ok(Self::Hooks),
                "COMMANDS" => Ok(Self::Commands),
                "SESSIONS" => Ok(Self::Sessions),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for ExternalAgentConfigMigrationItemType {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for ExternalAgentConfigMigrationItemType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for ExternalAgentConfigMigrationItemType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "Deprecated legacy notification for `apply_patch` textual output.\n\nThe server no longer emits this notification."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"FileChangeOutputDeltaNotification\","]
    #[doc = "  \"description\": \"Deprecated legacy notification for `apply_patch` textual output.\\n\\nThe server no longer emits this notification.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"delta\","]
    #[doc = "    \"itemId\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"delta\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"itemId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct FileChangeOutputDeltaNotification {
        pub delta: ::std::string::String,
        #[serde(rename = "itemId")]
        pub item_id: ::std::string::String,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`FileChangePatchUpdatedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"FileChangePatchUpdatedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"changes\","]
    #[doc = "    \"itemId\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"changes\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"$ref\": \"#/definitions/FileUpdateChange\""]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    \"itemId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct FileChangePatchUpdatedNotification {
        pub changes: ::std::vec::Vec<FileUpdateChange>,
        #[serde(rename = "itemId")]
        pub item_id: ::std::string::String,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`FileSystemAccessMode`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"read\","]
    #[doc = "    \"write\","]
    #[doc = "    \"deny\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum FileSystemAccessMode {
        #[serde(rename = "read")]
        Read,
        #[serde(rename = "write")]
        Write,
        #[serde(rename = "deny")]
        Deny,
    }
    impl ::std::fmt::Display for FileSystemAccessMode {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Read => f.write_str("read"),
                Self::Write => f.write_str("write"),
                Self::Deny => f.write_str("deny"),
            }
        }
    }
    impl ::std::str::FromStr for FileSystemAccessMode {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "read" => Ok(Self::Read),
                "write" => Ok(Self::Write),
                "deny" => Ok(Self::Deny),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for FileSystemAccessMode {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for FileSystemAccessMode {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for FileSystemAccessMode {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`FileSystemPath`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"PathFileSystemPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"path\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"$ref\": \"#/definitions/LegacyAppPathString\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"PathFileSystemPathType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"path\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"GlobPatternFileSystemPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"pattern\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"pattern\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"GlobPatternFileSystemPathType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"glob_pattern\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SpecialFileSystemPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\","]
    #[doc = "        \"value\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"SpecialFileSystemPathType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"special\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"value\": {"]
    #[doc = "          \"$ref\": \"#/definitions/FileSystemSpecialPath\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type")]
    pub enum FileSystemPath {
        #[doc = "PathFileSystemPath"]
        #[serde(rename = "path")]
        Path { path: LegacyAppPathString },
        #[doc = "GlobPatternFileSystemPath"]
        #[serde(rename = "glob_pattern")]
        GlobPattern { pattern: ::std::string::String },
        #[doc = "SpecialFileSystemPath"]
        #[serde(rename = "special")]
        Special { value: FileSystemSpecialPath },
    }
    #[doc = "`FileSystemSandboxEntry`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"access\","]
    #[doc = "    \"path\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"access\": {"]
    #[doc = "      \"$ref\": \"#/definitions/FileSystemAccessMode\""]
    #[doc = "    },"]
    #[doc = "    \"path\": {"]
    #[doc = "      \"$ref\": \"#/definitions/FileSystemPath\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct FileSystemSandboxEntry {
        pub access: FileSystemAccessMode,
        pub path: FileSystemPath,
    }
    #[doc = "`FileSystemSpecialPath`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"RootFileSystemSpecialPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"kind\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"root\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"MinimalFileSystemSpecialPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"kind\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"minimal\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"KindFileSystemSpecialPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"kind\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"project_roots\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"subpath\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"TmpdirFileSystemSpecialPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"kind\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"tmpdir\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SlashTmpFileSystemSpecialPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"kind\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"slash_tmp\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"kind\","]
    #[doc = "        \"path\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"unknown\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"subpath\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "kind")]
    pub enum FileSystemSpecialPath {
        #[serde(rename = "root")]
        Root,
        #[serde(rename = "minimal")]
        Minimal,
        #[doc = "KindFileSystemSpecialPath"]
        #[serde(rename = "project_roots")]
        ProjectRoots {
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            subpath: ::std::option::Option<::std::option::Option<::std::string::String>>,
        },
        #[serde(rename = "tmpdir")]
        Tmpdir,
        #[serde(rename = "slash_tmp")]
        SlashTmp,
        #[serde(rename = "unknown")]
        Unknown {
            path: ::std::string::String,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            subpath: ::std::option::Option<::std::option::Option<::std::string::String>>,
        },
    }
    #[doc = "`FileUpdateChange`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"diff\","]
    #[doc = "    \"kind\","]
    #[doc = "    \"path\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"diff\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"kind\": {"]
    #[doc = "      \"$ref\": \"#/definitions/PatchChangeKind\""]
    #[doc = "    },"]
    #[doc = "    \"path\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct FileUpdateChange {
        pub diff: ::std::string::String,
        pub kind: PatchChangeKind,
        pub path: ::std::string::String,
    }
    #[doc = "Filesystem watch notification emitted for `fs/watch` subscribers."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"FsChangedNotification\","]
    #[doc = "  \"description\": \"Filesystem watch notification emitted for `fs/watch` subscribers.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"changedPaths\","]
    #[doc = "    \"watchId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"changedPaths\": {"]
    #[doc = "      \"description\": \"File or directory paths associated with this event.\","]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"$ref\": \"#/definitions/AbsolutePathBuf\""]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    \"watchId\": {"]
    #[doc = "      \"description\": \"Watch identifier previously provided to `fs/watch`.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct FsChangedNotification {
        #[doc = "File or directory paths associated with this event."]
        #[serde(rename = "changedPaths")]
        pub changed_paths: ::std::vec::Vec<AbsolutePathBuf>,
        #[doc = "Watch identifier previously provided to `fs/watch`."]
        #[serde(rename = "watchId")]
        pub watch_id: ::std::string::String,
    }
    #[doc = "`FuzzyFileSearchMatchType`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"file\","]
    #[doc = "    \"directory\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum FuzzyFileSearchMatchType {
        #[serde(rename = "file")]
        File,
        #[serde(rename = "directory")]
        Directory,
    }
    impl ::std::fmt::Display for FuzzyFileSearchMatchType {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::File => f.write_str("file"),
                Self::Directory => f.write_str("directory"),
            }
        }
    }
    impl ::std::str::FromStr for FuzzyFileSearchMatchType {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "file" => Ok(Self::File),
                "directory" => Ok(Self::Directory),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for FuzzyFileSearchMatchType {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for FuzzyFileSearchMatchType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for FuzzyFileSearchMatchType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "Superset of [`codex_file_search::FileMatch`]"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"Superset of [`codex_file_search::FileMatch`]\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"file_name\","]
    #[doc = "    \"match_type\","]
    #[doc = "    \"path\","]
    #[doc = "    \"root\","]
    #[doc = "    \"score\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"file_name\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"indices\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"type\": \"integer\","]
    #[doc = "            \"format\": \"uint32\","]
    #[doc = "            \"minimum\": 0.0"]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"match_type\": {"]
    #[doc = "      \"$ref\": \"#/definitions/FuzzyFileSearchMatchType\""]
    #[doc = "    },"]
    #[doc = "    \"path\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"root\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"score\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"uint32\","]
    #[doc = "      \"minimum\": 0.0"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct FuzzyFileSearchResult {
        pub file_name: ::std::string::String,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub indices: ::std::option::Option<::std::vec::Vec<u32>>,
        pub match_type: FuzzyFileSearchMatchType,
        pub path: ::std::string::String,
        pub root: ::std::string::String,
        pub score: u32,
    }
    #[doc = "`FuzzyFileSearchSessionCompletedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"FuzzyFileSearchSessionCompletedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"sessionId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"sessionId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct FuzzyFileSearchSessionCompletedNotification {
        #[serde(rename = "sessionId")]
        pub session_id: ::std::string::String,
    }
    #[doc = "`FuzzyFileSearchSessionUpdatedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"FuzzyFileSearchSessionUpdatedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"files\","]
    #[doc = "    \"query\","]
    #[doc = "    \"sessionId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"files\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"$ref\": \"#/definitions/FuzzyFileSearchResult\""]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    \"query\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"sessionId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct FuzzyFileSearchSessionUpdatedNotification {
        pub files: ::std::vec::Vec<FuzzyFileSearchResult>,
        pub query: ::std::string::String,
        #[serde(rename = "sessionId")]
        pub session_id: ::std::string::String,
    }
    #[doc = "`GitInfo`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"branch\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"originUrl\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"sha\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct GitInfo {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub branch: ::std::option::Option<::std::string::String>,
        #[serde(
            rename = "originUrl",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub origin_url: ::std::option::Option<::std::string::String>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub sha: ::std::option::Option<::std::string::String>,
    }
    impl ::std::default::Default for GitInfo {
        fn default() -> Self {
            Self {
                branch: Default::default(),
                origin_url: Default::default(),
                sha: Default::default(),
            }
        }
    }
    #[doc = "[UNSTABLE] Temporary approval auto-review payload used by `item/autoApprovalReview/*` notifications. This shape is expected to change soon."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"[UNSTABLE] Temporary approval auto-review payload used by `item/autoApprovalReview/*` notifications. This shape is expected to change soon.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"status\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"rationale\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"riskLevel\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/GuardianRiskLevel\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"status\": {"]
    #[doc = "      \"$ref\": \"#/definitions/GuardianApprovalReviewStatus\""]
    #[doc = "    },"]
    #[doc = "    \"userAuthorization\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/GuardianUserAuthorization\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct GuardianApprovalReview {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub rationale: ::std::option::Option<::std::option::Option<::std::string::String>>,
        #[serde(
            rename = "riskLevel",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub risk_level: ::std::option::Option<::std::option::Option<GuardianRiskLevel>>,
        pub status: GuardianApprovalReviewStatus,
        #[serde(
            rename = "userAuthorization",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub user_authorization:
            ::std::option::Option<::std::option::Option<GuardianUserAuthorization>>,
    }
    #[doc = "`GuardianApprovalReviewAction`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"CommandGuardianApprovalReviewAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"command\","]
    #[doc = "        \"cwd\","]
    #[doc = "        \"source\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"command\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"cwd\": {"]
    #[doc = "          \"$ref\": \"#/definitions/AbsolutePathBuf\""]
    #[doc = "        },"]
    #[doc = "        \"source\": {"]
    #[doc = "          \"$ref\": \"#/definitions/GuardianCommandSource\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"CommandGuardianApprovalReviewActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"command\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ExecveGuardianApprovalReviewAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"argv\","]
    #[doc = "        \"cwd\","]
    #[doc = "        \"program\","]
    #[doc = "        \"source\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"argv\": {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"type\": \"string\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"cwd\": {"]
    #[doc = "          \"$ref\": \"#/definitions/AbsolutePathBuf\""]
    #[doc = "        },"]
    #[doc = "        \"program\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"source\": {"]
    #[doc = "          \"$ref\": \"#/definitions/GuardianCommandSource\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ExecveGuardianApprovalReviewActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"execve\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ApplyPatchGuardianApprovalReviewAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"cwd\","]
    #[doc = "        \"files\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"cwd\": {"]
    #[doc = "          \"$ref\": \"#/definitions/AbsolutePathBuf\""]
    #[doc = "        },"]
    #[doc = "        \"files\": {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/AbsolutePathBuf\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ApplyPatchGuardianApprovalReviewActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"applyPatch\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"NetworkAccessGuardianApprovalReviewAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"host\","]
    #[doc = "        \"port\","]
    #[doc = "        \"protocol\","]
    #[doc = "        \"target\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"host\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"port\": {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"uint16\","]
    #[doc = "          \"minimum\": 0.0"]
    #[doc = "        },"]
    #[doc = "        \"protocol\": {"]
    #[doc = "          \"$ref\": \"#/definitions/NetworkApprovalProtocol\""]
    #[doc = "        },"]
    #[doc = "        \"target\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"NetworkAccessGuardianApprovalReviewActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"networkAccess\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"McpToolCallGuardianApprovalReviewAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"server\","]
    #[doc = "        \"toolName\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"connectorId\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"connectorName\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"server\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"toolName\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"toolTitle\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"McpToolCallGuardianApprovalReviewActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"mcpToolCall\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"RequestPermissionsGuardianApprovalReviewAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"permissions\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"permissions\": {"]
    #[doc = "          \"$ref\": \"#/definitions/RequestPermissionProfile\""]
    #[doc = "        },"]
    #[doc = "        \"reason\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"RequestPermissionsGuardianApprovalReviewActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"requestPermissions\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type")]
    pub enum GuardianApprovalReviewAction {
        #[doc = "CommandGuardianApprovalReviewAction"]
        #[serde(rename = "command")]
        Command {
            command: ::std::string::String,
            cwd: AbsolutePathBuf,
            source: GuardianCommandSource,
        },
        #[doc = "ExecveGuardianApprovalReviewAction"]
        #[serde(rename = "execve")]
        Execve {
            argv: ::std::vec::Vec<::std::string::String>,
            cwd: AbsolutePathBuf,
            program: ::std::string::String,
            source: GuardianCommandSource,
        },
        #[doc = "ApplyPatchGuardianApprovalReviewAction"]
        #[serde(rename = "applyPatch")]
        ApplyPatch {
            cwd: AbsolutePathBuf,
            files: ::std::vec::Vec<AbsolutePathBuf>,
        },
        #[doc = "NetworkAccessGuardianApprovalReviewAction"]
        #[serde(rename = "networkAccess")]
        NetworkAccess {
            host: ::std::string::String,
            port: u16,
            protocol: NetworkApprovalProtocol,
            target: ::std::string::String,
        },
        #[doc = "McpToolCallGuardianApprovalReviewAction"]
        #[serde(rename = "mcpToolCall")]
        McpToolCall {
            #[serde(
                rename = "connectorId",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            connector_id: ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[serde(
                rename = "connectorName",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            connector_name: ::std::option::Option<::std::option::Option<::std::string::String>>,
            server: ::std::string::String,
            #[serde(rename = "toolName")]
            tool_name: ::std::string::String,
            #[serde(
                rename = "toolTitle",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            tool_title: ::std::option::Option<::std::option::Option<::std::string::String>>,
        },
        #[doc = "RequestPermissionsGuardianApprovalReviewAction"]
        #[serde(rename = "requestPermissions")]
        RequestPermissions {
            permissions: RequestPermissionProfile,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            reason: ::std::option::Option<::std::option::Option<::std::string::String>>,
        },
    }
    #[doc = "[UNSTABLE] Lifecycle state for an approval auto-review."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"[UNSTABLE] Lifecycle state for an approval auto-review.\","]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"inProgress\","]
    #[doc = "    \"approved\","]
    #[doc = "    \"denied\","]
    #[doc = "    \"timedOut\","]
    #[doc = "    \"aborted\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum GuardianApprovalReviewStatus {
        #[serde(rename = "inProgress")]
        InProgress,
        #[serde(rename = "approved")]
        Approved,
        #[serde(rename = "denied")]
        Denied,
        #[serde(rename = "timedOut")]
        TimedOut,
        #[serde(rename = "aborted")]
        Aborted,
    }
    impl ::std::fmt::Display for GuardianApprovalReviewStatus {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::InProgress => f.write_str("inProgress"),
                Self::Approved => f.write_str("approved"),
                Self::Denied => f.write_str("denied"),
                Self::TimedOut => f.write_str("timedOut"),
                Self::Aborted => f.write_str("aborted"),
            }
        }
    }
    impl ::std::str::FromStr for GuardianApprovalReviewStatus {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "inProgress" => Ok(Self::InProgress),
                "approved" => Ok(Self::Approved),
                "denied" => Ok(Self::Denied),
                "timedOut" => Ok(Self::TimedOut),
                "aborted" => Ok(Self::Aborted),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for GuardianApprovalReviewStatus {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for GuardianApprovalReviewStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for GuardianApprovalReviewStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`GuardianCommandSource`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"shell\","]
    #[doc = "    \"unifiedExec\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum GuardianCommandSource {
        #[serde(rename = "shell")]
        Shell,
        #[serde(rename = "unifiedExec")]
        UnifiedExec,
    }
    impl ::std::fmt::Display for GuardianCommandSource {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Shell => f.write_str("shell"),
                Self::UnifiedExec => f.write_str("unifiedExec"),
            }
        }
    }
    impl ::std::str::FromStr for GuardianCommandSource {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "shell" => Ok(Self::Shell),
                "unifiedExec" => Ok(Self::UnifiedExec),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for GuardianCommandSource {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for GuardianCommandSource {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for GuardianCommandSource {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "[UNSTABLE] Risk level assigned by approval auto-review."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"[UNSTABLE] Risk level assigned by approval auto-review.\","]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"low\","]
    #[doc = "    \"medium\","]
    #[doc = "    \"high\","]
    #[doc = "    \"critical\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum GuardianRiskLevel {
        #[serde(rename = "low")]
        Low,
        #[serde(rename = "medium")]
        Medium,
        #[serde(rename = "high")]
        High,
        #[serde(rename = "critical")]
        Critical,
    }
    impl ::std::fmt::Display for GuardianRiskLevel {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Low => f.write_str("low"),
                Self::Medium => f.write_str("medium"),
                Self::High => f.write_str("high"),
                Self::Critical => f.write_str("critical"),
            }
        }
    }
    impl ::std::str::FromStr for GuardianRiskLevel {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "low" => Ok(Self::Low),
                "medium" => Ok(Self::Medium),
                "high" => Ok(Self::High),
                "critical" => Ok(Self::Critical),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for GuardianRiskLevel {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for GuardianRiskLevel {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for GuardianRiskLevel {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "[UNSTABLE] Authorization level assigned by approval auto-review."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"[UNSTABLE] Authorization level assigned by approval auto-review.\","]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"unknown\","]
    #[doc = "    \"low\","]
    #[doc = "    \"medium\","]
    #[doc = "    \"high\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum GuardianUserAuthorization {
        #[serde(rename = "unknown")]
        Unknown,
        #[serde(rename = "low")]
        Low,
        #[serde(rename = "medium")]
        Medium,
        #[serde(rename = "high")]
        High,
    }
    impl ::std::fmt::Display for GuardianUserAuthorization {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Unknown => f.write_str("unknown"),
                Self::Low => f.write_str("low"),
                Self::Medium => f.write_str("medium"),
                Self::High => f.write_str("high"),
            }
        }
    }
    impl ::std::str::FromStr for GuardianUserAuthorization {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "unknown" => Ok(Self::Unknown),
                "low" => Ok(Self::Low),
                "medium" => Ok(Self::Medium),
                "high" => Ok(Self::High),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for GuardianUserAuthorization {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for GuardianUserAuthorization {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for GuardianUserAuthorization {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`GuardianWarningNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"GuardianWarningNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"message\","]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"message\": {"]
    #[doc = "      \"description\": \"Concise guardian warning message for the user.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"description\": \"Thread target for the guardian warning.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct GuardianWarningNotification {
        #[doc = "Concise guardian warning message for the user."]
        pub message: ::std::string::String,
        #[doc = "Thread target for the guardian warning."]
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
    }
    #[doc = "`HookCompletedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"HookCompletedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"run\","]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"run\": {"]
    #[doc = "      \"$ref\": \"#/definitions/HookRunSummary\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct HookCompletedNotification {
        pub run: HookRunSummary,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(
            rename = "turnId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub turn_id: ::std::option::Option<::std::string::String>,
    }
    #[doc = "`HookEventName`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"preToolUse\","]
    #[doc = "    \"permissionRequest\","]
    #[doc = "    \"postToolUse\","]
    #[doc = "    \"preCompact\","]
    #[doc = "    \"postCompact\","]
    #[doc = "    \"sessionStart\","]
    #[doc = "    \"userPromptSubmit\","]
    #[doc = "    \"subagentStart\","]
    #[doc = "    \"subagentStop\","]
    #[doc = "    \"stop\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum HookEventName {
        #[serde(rename = "preToolUse")]
        PreToolUse,
        #[serde(rename = "permissionRequest")]
        PermissionRequest,
        #[serde(rename = "postToolUse")]
        PostToolUse,
        #[serde(rename = "preCompact")]
        PreCompact,
        #[serde(rename = "postCompact")]
        PostCompact,
        #[serde(rename = "sessionStart")]
        SessionStart,
        #[serde(rename = "userPromptSubmit")]
        UserPromptSubmit,
        #[serde(rename = "subagentStart")]
        SubagentStart,
        #[serde(rename = "subagentStop")]
        SubagentStop,
        #[serde(rename = "stop")]
        Stop,
    }
    impl ::std::fmt::Display for HookEventName {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::PreToolUse => f.write_str("preToolUse"),
                Self::PermissionRequest => f.write_str("permissionRequest"),
                Self::PostToolUse => f.write_str("postToolUse"),
                Self::PreCompact => f.write_str("preCompact"),
                Self::PostCompact => f.write_str("postCompact"),
                Self::SessionStart => f.write_str("sessionStart"),
                Self::UserPromptSubmit => f.write_str("userPromptSubmit"),
                Self::SubagentStart => f.write_str("subagentStart"),
                Self::SubagentStop => f.write_str("subagentStop"),
                Self::Stop => f.write_str("stop"),
            }
        }
    }
    impl ::std::str::FromStr for HookEventName {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "preToolUse" => Ok(Self::PreToolUse),
                "permissionRequest" => Ok(Self::PermissionRequest),
                "postToolUse" => Ok(Self::PostToolUse),
                "preCompact" => Ok(Self::PreCompact),
                "postCompact" => Ok(Self::PostCompact),
                "sessionStart" => Ok(Self::SessionStart),
                "userPromptSubmit" => Ok(Self::UserPromptSubmit),
                "subagentStart" => Ok(Self::SubagentStart),
                "subagentStop" => Ok(Self::SubagentStop),
                "stop" => Ok(Self::Stop),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for HookEventName {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for HookEventName {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for HookEventName {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`HookExecutionMode`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"sync\","]
    #[doc = "    \"async\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum HookExecutionMode {
        #[serde(rename = "sync")]
        Sync,
        #[serde(rename = "async")]
        Async,
    }
    impl ::std::fmt::Display for HookExecutionMode {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Sync => f.write_str("sync"),
                Self::Async => f.write_str("async"),
            }
        }
    }
    impl ::std::str::FromStr for HookExecutionMode {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "sync" => Ok(Self::Sync),
                "async" => Ok(Self::Async),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for HookExecutionMode {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for HookExecutionMode {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for HookExecutionMode {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`HookHandlerType`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"command\","]
    #[doc = "    \"prompt\","]
    #[doc = "    \"agent\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum HookHandlerType {
        #[serde(rename = "command")]
        Command,
        #[serde(rename = "prompt")]
        Prompt,
        #[serde(rename = "agent")]
        Agent,
    }
    impl ::std::fmt::Display for HookHandlerType {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Command => f.write_str("command"),
                Self::Prompt => f.write_str("prompt"),
                Self::Agent => f.write_str("agent"),
            }
        }
    }
    impl ::std::str::FromStr for HookHandlerType {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "command" => Ok(Self::Command),
                "prompt" => Ok(Self::Prompt),
                "agent" => Ok(Self::Agent),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for HookHandlerType {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for HookHandlerType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for HookHandlerType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`HookOutputEntry`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"kind\","]
    #[doc = "    \"text\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"kind\": {"]
    #[doc = "      \"$ref\": \"#/definitions/HookOutputEntryKind\""]
    #[doc = "    },"]
    #[doc = "    \"text\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct HookOutputEntry {
        pub kind: HookOutputEntryKind,
        pub text: ::std::string::String,
    }
    #[doc = "`HookOutputEntryKind`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"warning\","]
    #[doc = "    \"stop\","]
    #[doc = "    \"feedback\","]
    #[doc = "    \"context\","]
    #[doc = "    \"error\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum HookOutputEntryKind {
        #[serde(rename = "warning")]
        Warning,
        #[serde(rename = "stop")]
        Stop,
        #[serde(rename = "feedback")]
        Feedback,
        #[serde(rename = "context")]
        Context,
        #[serde(rename = "error")]
        Error,
    }
    impl ::std::fmt::Display for HookOutputEntryKind {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Warning => f.write_str("warning"),
                Self::Stop => f.write_str("stop"),
                Self::Feedback => f.write_str("feedback"),
                Self::Context => f.write_str("context"),
                Self::Error => f.write_str("error"),
            }
        }
    }
    impl ::std::str::FromStr for HookOutputEntryKind {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "warning" => Ok(Self::Warning),
                "stop" => Ok(Self::Stop),
                "feedback" => Ok(Self::Feedback),
                "context" => Ok(Self::Context),
                "error" => Ok(Self::Error),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for HookOutputEntryKind {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for HookOutputEntryKind {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for HookOutputEntryKind {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`HookPromptFragment`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"hookRunId\","]
    #[doc = "    \"text\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"hookRunId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"text\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct HookPromptFragment {
        #[serde(rename = "hookRunId")]
        pub hook_run_id: ::std::string::String,
        pub text: ::std::string::String,
    }
    #[doc = "`HookRunStatus`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"running\","]
    #[doc = "    \"completed\","]
    #[doc = "    \"failed\","]
    #[doc = "    \"blocked\","]
    #[doc = "    \"stopped\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum HookRunStatus {
        #[serde(rename = "running")]
        Running,
        #[serde(rename = "completed")]
        Completed,
        #[serde(rename = "failed")]
        Failed,
        #[serde(rename = "blocked")]
        Blocked,
        #[serde(rename = "stopped")]
        Stopped,
    }
    impl ::std::fmt::Display for HookRunStatus {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Running => f.write_str("running"),
                Self::Completed => f.write_str("completed"),
                Self::Failed => f.write_str("failed"),
                Self::Blocked => f.write_str("blocked"),
                Self::Stopped => f.write_str("stopped"),
            }
        }
    }
    impl ::std::str::FromStr for HookRunStatus {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "running" => Ok(Self::Running),
                "completed" => Ok(Self::Completed),
                "failed" => Ok(Self::Failed),
                "blocked" => Ok(Self::Blocked),
                "stopped" => Ok(Self::Stopped),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for HookRunStatus {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for HookRunStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for HookRunStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`HookRunSummary`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"displayOrder\","]
    #[doc = "    \"entries\","]
    #[doc = "    \"eventName\","]
    #[doc = "    \"executionMode\","]
    #[doc = "    \"handlerType\","]
    #[doc = "    \"id\","]
    #[doc = "    \"scope\","]
    #[doc = "    \"sourcePath\","]
    #[doc = "    \"startedAt\","]
    #[doc = "    \"status\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"completedAt\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"int64\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"displayOrder\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"durationMs\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"int64\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"entries\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"$ref\": \"#/definitions/HookOutputEntry\""]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    \"eventName\": {"]
    #[doc = "      \"$ref\": \"#/definitions/HookEventName\""]
    #[doc = "    },"]
    #[doc = "    \"executionMode\": {"]
    #[doc = "      \"$ref\": \"#/definitions/HookExecutionMode\""]
    #[doc = "    },"]
    #[doc = "    \"handlerType\": {"]
    #[doc = "      \"$ref\": \"#/definitions/HookHandlerType\""]
    #[doc = "    },"]
    #[doc = "    \"id\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"scope\": {"]
    #[doc = "      \"$ref\": \"#/definitions/HookScope\""]
    #[doc = "    },"]
    #[doc = "    \"source\": {"]
    #[doc = "      \"default\": \"unknown\","]
    #[doc = "      \"allOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/HookSource\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"sourcePath\": {"]
    #[doc = "      \"$ref\": \"#/definitions/AbsolutePathBuf\""]
    #[doc = "    },"]
    #[doc = "    \"startedAt\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"status\": {"]
    #[doc = "      \"$ref\": \"#/definitions/HookRunStatus\""]
    #[doc = "    },"]
    #[doc = "    \"statusMessage\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct HookRunSummary {
        #[serde(
            rename = "completedAt",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[ts(type = "number | null")]
        pub completed_at: ::std::option::Option<i64>,
        #[serde(rename = "displayOrder")]
        #[ts(type = "number")]
        pub display_order: i64,
        #[serde(
            rename = "durationMs",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[ts(type = "number | null")]
        pub duration_ms: ::std::option::Option<i64>,
        pub entries: ::std::vec::Vec<HookOutputEntry>,
        #[serde(rename = "eventName")]
        pub event_name: HookEventName,
        #[serde(rename = "executionMode")]
        pub execution_mode: HookExecutionMode,
        #[serde(rename = "handlerType")]
        pub handler_type: HookHandlerType,
        pub id: ::std::string::String,
        pub scope: HookScope,
        #[serde(default = "defaults::hook_run_summary_source")]
        pub source: HookSource,
        #[serde(rename = "sourcePath")]
        pub source_path: AbsolutePathBuf,
        #[serde(rename = "startedAt")]
        #[ts(type = "number")]
        pub started_at: i64,
        pub status: HookRunStatus,
        #[serde(
            rename = "statusMessage",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub status_message: ::std::option::Option<::std::string::String>,
    }
    #[doc = "`HookScope`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"thread\","]
    #[doc = "    \"turn\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum HookScope {
        #[serde(rename = "thread")]
        Thread,
        #[serde(rename = "turn")]
        Turn,
    }
    impl ::std::fmt::Display for HookScope {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Thread => f.write_str("thread"),
                Self::Turn => f.write_str("turn"),
            }
        }
    }
    impl ::std::str::FromStr for HookScope {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "thread" => Ok(Self::Thread),
                "turn" => Ok(Self::Turn),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for HookScope {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for HookScope {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for HookScope {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`HookSource`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"system\","]
    #[doc = "    \"user\","]
    #[doc = "    \"project\","]
    #[doc = "    \"mdm\","]
    #[doc = "    \"sessionFlags\","]
    #[doc = "    \"plugin\","]
    #[doc = "    \"cloudRequirements\","]
    #[doc = "    \"cloudManagedConfig\","]
    #[doc = "    \"legacyManagedConfigFile\","]
    #[doc = "    \"legacyManagedConfigMdm\","]
    #[doc = "    \"unknown\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum HookSource {
        #[serde(rename = "system")]
        System,
        #[serde(rename = "user")]
        User,
        #[serde(rename = "project")]
        Project,
        #[serde(rename = "mdm")]
        Mdm,
        #[serde(rename = "sessionFlags")]
        SessionFlags,
        #[serde(rename = "plugin")]
        Plugin,
        #[serde(rename = "cloudRequirements")]
        CloudRequirements,
        #[serde(rename = "cloudManagedConfig")]
        CloudManagedConfig,
        #[serde(rename = "legacyManagedConfigFile")]
        LegacyManagedConfigFile,
        #[serde(rename = "legacyManagedConfigMdm")]
        LegacyManagedConfigMdm,
        #[serde(rename = "unknown")]
        Unknown,
    }
    impl ::std::fmt::Display for HookSource {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::System => f.write_str("system"),
                Self::User => f.write_str("user"),
                Self::Project => f.write_str("project"),
                Self::Mdm => f.write_str("mdm"),
                Self::SessionFlags => f.write_str("sessionFlags"),
                Self::Plugin => f.write_str("plugin"),
                Self::CloudRequirements => f.write_str("cloudRequirements"),
                Self::CloudManagedConfig => f.write_str("cloudManagedConfig"),
                Self::LegacyManagedConfigFile => f.write_str("legacyManagedConfigFile"),
                Self::LegacyManagedConfigMdm => f.write_str("legacyManagedConfigMdm"),
                Self::Unknown => f.write_str("unknown"),
            }
        }
    }
    impl ::std::str::FromStr for HookSource {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "system" => Ok(Self::System),
                "user" => Ok(Self::User),
                "project" => Ok(Self::Project),
                "mdm" => Ok(Self::Mdm),
                "sessionFlags" => Ok(Self::SessionFlags),
                "plugin" => Ok(Self::Plugin),
                "cloudRequirements" => Ok(Self::CloudRequirements),
                "cloudManagedConfig" => Ok(Self::CloudManagedConfig),
                "legacyManagedConfigFile" => Ok(Self::LegacyManagedConfigFile),
                "legacyManagedConfigMdm" => Ok(Self::LegacyManagedConfigMdm),
                "unknown" => Ok(Self::Unknown),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for HookSource {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for HookSource {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for HookSource {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`HookStartedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"HookStartedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"run\","]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"run\": {"]
    #[doc = "      \"$ref\": \"#/definitions/HookRunSummary\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct HookStartedNotification {
        pub run: HookRunSummary,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(
            rename = "turnId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub turn_id: ::std::option::Option<::std::string::String>,
    }
    #[doc = "`ImageDetail`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"auto\","]
    #[doc = "    \"low\","]
    #[doc = "    \"high\","]
    #[doc = "    \"original\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum ImageDetail {
        #[serde(rename = "auto")]
        Auto,
        #[serde(rename = "low")]
        Low,
        #[serde(rename = "high")]
        High,
        #[serde(rename = "original")]
        Original,
    }
    impl ::std::fmt::Display for ImageDetail {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Auto => f.write_str("auto"),
                Self::Low => f.write_str("low"),
                Self::High => f.write_str("high"),
                Self::Original => f.write_str("original"),
            }
        }
    }
    impl ::std::str::FromStr for ImageDetail {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "auto" => Ok(Self::Auto),
                "low" => Ok(Self::Low),
                "high" => Ok(Self::High),
                "original" => Ok(Self::Original),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for ImageDetail {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for ImageDetail {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for ImageDetail {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`ItemCompletedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ItemCompletedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"completedAtMs\","]
    #[doc = "    \"item\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"completedAtMs\": {"]
    #[doc = "      \"description\": \"Unix timestamp (in milliseconds) when this item lifecycle completed.\","]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"item\": {"]
    #[doc = "      \"$ref\": \"#/definitions/ThreadItem\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ItemCompletedNotification {
        #[doc = "Unix timestamp (in milliseconds) when this item lifecycle completed."]
        #[serde(rename = "completedAtMs")]
        #[ts(type = "number")]
        pub completed_at_ms: i64,
        pub item: ThreadItem,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "[UNSTABLE] Temporary notification payload for approval auto-review. This shape is expected to change soon."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ItemGuardianApprovalReviewCompletedNotification\","]
    #[doc = "  \"description\": \"[UNSTABLE] Temporary notification payload for approval auto-review. This shape is expected to change soon.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"action\","]
    #[doc = "    \"completedAtMs\","]
    #[doc = "    \"decisionSource\","]
    #[doc = "    \"review\","]
    #[doc = "    \"reviewId\","]
    #[doc = "    \"startedAtMs\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"action\": {"]
    #[doc = "      \"$ref\": \"#/definitions/GuardianApprovalReviewAction\""]
    #[doc = "    },"]
    #[doc = "    \"completedAtMs\": {"]
    #[doc = "      \"description\": \"Unix timestamp (in milliseconds) when this review completed.\","]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"decisionSource\": {"]
    #[doc = "      \"$ref\": \"#/definitions/AutoReviewDecisionSource\""]
    #[doc = "    },"]
    #[doc = "    \"review\": {"]
    #[doc = "      \"$ref\": \"#/definitions/GuardianApprovalReview\""]
    #[doc = "    },"]
    #[doc = "    \"reviewId\": {"]
    #[doc = "      \"description\": \"Stable identifier for this review.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"startedAtMs\": {"]
    #[doc = "      \"description\": \"Unix timestamp (in milliseconds) when this review started.\","]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"targetItemId\": {"]
    #[doc = "      \"description\": \"Identifier for the reviewed item or tool call when one exists.\\n\\nIn most cases, one review maps to one target item. The exceptions are - execve reviews, where a single command may contain multiple execve calls to review (only possible when using the shell_zsh_fork feature) - network policy reviews, where there is no target item\\n\\nA network call is triggered by a CommandExecution item, so having a target_item_id set to the CommandExecution item would be misleading because the review is about the network call, not the command execution. Therefore, target_item_id is set to None for network policy reviews.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Identifier for the reviewed item or tool call when one exists.\\n\\nIn most cases, one review maps to one target item. The exceptions are - execve reviews, where a single command may contain multiple execve calls to review (only possible when using the shell_zsh_fork feature) - network policy reviews, where there is no target item\\n\\nA network call is triggered by a CommandExecution item, so having a target_item_id set to the CommandExecution item would be misleading because the review is about the network call, not the command execution. Therefore, target_item_id is set to None for network policy reviews.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ItemGuardianApprovalReviewCompletedNotification {
        pub action: GuardianApprovalReviewAction,
        #[doc = "Unix timestamp (in milliseconds) when this review completed."]
        #[serde(rename = "completedAtMs")]
        #[ts(type = "number")]
        pub completed_at_ms: i64,
        #[serde(rename = "decisionSource")]
        pub decision_source: AutoReviewDecisionSource,
        pub review: GuardianApprovalReview,
        #[doc = "Stable identifier for this review."]
        #[serde(rename = "reviewId")]
        pub review_id: ::std::string::String,
        #[doc = "Unix timestamp (in milliseconds) when this review started."]
        #[serde(rename = "startedAtMs")]
        #[ts(type = "number")]
        pub started_at_ms: i64,
        #[doc = "Identifier for the reviewed item or tool call when one exists.\n\nIn most cases, one review maps to one target item. The exceptions are - execve reviews, where a single command may contain multiple execve calls to review (only possible when using the shell_zsh_fork feature) - network policy reviews, where there is no target item\n\nA network call is triggered by a CommandExecution item, so having a target_item_id set to the CommandExecution item would be misleading because the review is about the network call, not the command execution. Therefore, target_item_id is set to None for network policy reviews."]
        #[serde(
            rename = "targetItemId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub target_item_id: ::std::option::Option<::std::option::Option<::std::string::String>>,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "[UNSTABLE] Temporary notification payload for approval auto-review. This shape is expected to change soon."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ItemGuardianApprovalReviewStartedNotification\","]
    #[doc = "  \"description\": \"[UNSTABLE] Temporary notification payload for approval auto-review. This shape is expected to change soon.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"action\","]
    #[doc = "    \"review\","]
    #[doc = "    \"reviewId\","]
    #[doc = "    \"startedAtMs\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"action\": {"]
    #[doc = "      \"$ref\": \"#/definitions/GuardianApprovalReviewAction\""]
    #[doc = "    },"]
    #[doc = "    \"review\": {"]
    #[doc = "      \"$ref\": \"#/definitions/GuardianApprovalReview\""]
    #[doc = "    },"]
    #[doc = "    \"reviewId\": {"]
    #[doc = "      \"description\": \"Stable identifier for this review.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"startedAtMs\": {"]
    #[doc = "      \"description\": \"Unix timestamp (in milliseconds) when this review started.\","]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"targetItemId\": {"]
    #[doc = "      \"description\": \"Identifier for the reviewed item or tool call when one exists.\\n\\nIn most cases, one review maps to one target item. The exceptions are - execve reviews, where a single command may contain multiple execve calls to review (only possible when using the shell_zsh_fork feature) - network policy reviews, where there is no target item\\n\\nA network call is triggered by a CommandExecution item, so having a target_item_id set to the CommandExecution item would be misleading because the review is about the network call, not the command execution. Therefore, target_item_id is set to None for network policy reviews.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Identifier for the reviewed item or tool call when one exists.\\n\\nIn most cases, one review maps to one target item. The exceptions are - execve reviews, where a single command may contain multiple execve calls to review (only possible when using the shell_zsh_fork feature) - network policy reviews, where there is no target item\\n\\nA network call is triggered by a CommandExecution item, so having a target_item_id set to the CommandExecution item would be misleading because the review is about the network call, not the command execution. Therefore, target_item_id is set to None for network policy reviews.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ItemGuardianApprovalReviewStartedNotification {
        pub action: GuardianApprovalReviewAction,
        pub review: GuardianApprovalReview,
        #[doc = "Stable identifier for this review."]
        #[serde(rename = "reviewId")]
        pub review_id: ::std::string::String,
        #[doc = "Unix timestamp (in milliseconds) when this review started."]
        #[serde(rename = "startedAtMs")]
        #[ts(type = "number")]
        pub started_at_ms: i64,
        #[doc = "Identifier for the reviewed item or tool call when one exists.\n\nIn most cases, one review maps to one target item. The exceptions are - execve reviews, where a single command may contain multiple execve calls to review (only possible when using the shell_zsh_fork feature) - network policy reviews, where there is no target item\n\nA network call is triggered by a CommandExecution item, so having a target_item_id set to the CommandExecution item would be misleading because the review is about the network call, not the command execution. Therefore, target_item_id is set to None for network policy reviews."]
        #[serde(
            rename = "targetItemId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub target_item_id: ::std::option::Option<::std::option::Option<::std::string::String>>,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`ItemStartedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ItemStartedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"item\","]
    #[doc = "    \"startedAtMs\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"item\": {"]
    #[doc = "      \"$ref\": \"#/definitions/ThreadItem\""]
    #[doc = "    },"]
    #[doc = "    \"startedAtMs\": {"]
    #[doc = "      \"description\": \"Unix timestamp (in milliseconds) when this item lifecycle started.\","]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ItemStartedNotification {
        pub item: ThreadItem,
        #[doc = "Unix timestamp (in milliseconds) when this item lifecycle started."]
        #[serde(rename = "startedAtMs")]
        #[ts(type = "number")]
        pub started_at_ms: i64,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`LegacyAppPathString`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    #[serde(transparent)]
    pub struct LegacyAppPathString(pub ::std::string::String);
    impl ::std::ops::Deref for LegacyAppPathString {
        type Target = ::std::string::String;
        fn deref(&self) -> &::std::string::String {
            &self.0
        }
    }
    impl ::std::convert::From<LegacyAppPathString> for ::std::string::String {
        fn from(value: LegacyAppPathString) -> Self {
            value.0
        }
    }
    impl ::std::convert::From<::std::string::String> for LegacyAppPathString {
        fn from(value: ::std::string::String) -> Self {
            Self(value)
        }
    }
    impl ::std::str::FromStr for LegacyAppPathString {
        type Err = ::std::convert::Infallible;
        fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
            Ok(Self(value.to_string()))
        }
    }
    impl ::std::fmt::Display for LegacyAppPathString {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            self.0.fmt(f)
        }
    }
    #[doc = "`McpServerOauthLoginCompletedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"McpServerOauthLoginCompletedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"name\","]
    #[doc = "    \"success\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"error\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"name\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"success\": {"]
    #[doc = "      \"type\": \"boolean\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct McpServerOauthLoginCompletedNotification {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub error: ::std::option::Option<::std::string::String>,
        pub name: ::std::string::String,
        pub success: bool,
        #[serde(
            rename = "threadId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub thread_id: ::std::option::Option<::std::string::String>,
    }
    #[doc = "`McpServerStartupFailureReason`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"reauthenticationRequired\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum McpServerStartupFailureReason {
        #[serde(rename = "reauthenticationRequired")]
        ReauthenticationRequired,
    }
    impl ::std::fmt::Display for McpServerStartupFailureReason {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::ReauthenticationRequired => f.write_str("reauthenticationRequired"),
            }
        }
    }
    impl ::std::str::FromStr for McpServerStartupFailureReason {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "reauthenticationRequired" => Ok(Self::ReauthenticationRequired),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for McpServerStartupFailureReason {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for McpServerStartupFailureReason {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for McpServerStartupFailureReason {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`McpServerStartupState`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"starting\","]
    #[doc = "    \"ready\","]
    #[doc = "    \"failed\","]
    #[doc = "    \"cancelled\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum McpServerStartupState {
        #[serde(rename = "starting")]
        Starting,
        #[serde(rename = "ready")]
        Ready,
        #[serde(rename = "failed")]
        Failed,
        #[serde(rename = "cancelled")]
        Cancelled,
    }
    impl ::std::fmt::Display for McpServerStartupState {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Starting => f.write_str("starting"),
                Self::Ready => f.write_str("ready"),
                Self::Failed => f.write_str("failed"),
                Self::Cancelled => f.write_str("cancelled"),
            }
        }
    }
    impl ::std::str::FromStr for McpServerStartupState {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "starting" => Ok(Self::Starting),
                "ready" => Ok(Self::Ready),
                "failed" => Ok(Self::Failed),
                "cancelled" => Ok(Self::Cancelled),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for McpServerStartupState {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for McpServerStartupState {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for McpServerStartupState {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`McpServerStatusUpdatedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"McpServerStatusUpdatedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"name\","]
    #[doc = "    \"status\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"error\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"failureReason\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/McpServerStartupFailureReason\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"name\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"status\": {"]
    #[doc = "      \"$ref\": \"#/definitions/McpServerStartupState\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct McpServerStatusUpdatedNotification {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub error: ::std::option::Option<::std::string::String>,
        #[serde(
            rename = "failureReason",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub failure_reason: ::std::option::Option<McpServerStartupFailureReason>,
        pub name: ::std::string::String,
        pub status: McpServerStartupState,
        #[serde(
            rename = "threadId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub thread_id: ::std::option::Option<::std::string::String>,
    }
    #[doc = "`McpToolCallAppContext`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"connectorId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"actionName\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"appName\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"connectorId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"linkId\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"resourceUri\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"templateId\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct McpToolCallAppContext {
        #[serde(
            rename = "actionName",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub action_name: ::std::option::Option<::std::option::Option<::std::string::String>>,
        #[serde(
            rename = "appName",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub app_name: ::std::option::Option<::std::option::Option<::std::string::String>>,
        #[serde(rename = "connectorId")]
        pub connector_id: ::std::string::String,
        #[serde(
            rename = "linkId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub link_id: ::std::option::Option<::std::option::Option<::std::string::String>>,
        #[serde(
            rename = "resourceUri",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub resource_uri: ::std::option::Option<::std::option::Option<::std::string::String>>,
        #[serde(
            rename = "templateId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub template_id: ::std::option::Option<::std::option::Option<::std::string::String>>,
    }
    #[doc = "`McpToolCallError`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"message\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"message\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct McpToolCallError {
        pub message: ::std::string::String,
    }
    #[doc = "`McpToolCallProgressNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"McpToolCallProgressNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"itemId\","]
    #[doc = "    \"message\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"itemId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"message\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct McpToolCallProgressNotification {
        #[serde(rename = "itemId")]
        pub item_id: ::std::string::String,
        pub message: ::std::string::String,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`McpToolCallResult`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"content\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"_meta\": true,"]
    #[doc = "    \"content\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": true"]
    #[doc = "    },"]
    #[doc = "    \"structuredContent\": true"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct McpToolCallResult {
        pub content: ::std::vec::Vec<::serde_json::Value>,
        #[serde(
            rename = "_meta",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub meta: ::std::option::Option<::serde_json::Value>,
        #[serde(
            rename = "structuredContent",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub structured_content: ::std::option::Option<::serde_json::Value>,
    }
    #[doc = "`McpToolCallStatus`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"inProgress\","]
    #[doc = "    \"completed\","]
    #[doc = "    \"failed\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum McpToolCallStatus {
        #[serde(rename = "inProgress")]
        InProgress,
        #[serde(rename = "completed")]
        Completed,
        #[serde(rename = "failed")]
        Failed,
    }
    impl ::std::fmt::Display for McpToolCallStatus {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::InProgress => f.write_str("inProgress"),
                Self::Completed => f.write_str("completed"),
                Self::Failed => f.write_str("failed"),
            }
        }
    }
    impl ::std::str::FromStr for McpToolCallStatus {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "inProgress" => Ok(Self::InProgress),
                "completed" => Ok(Self::Completed),
                "failed" => Ok(Self::Failed),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for McpToolCallStatus {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for McpToolCallStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for McpToolCallStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`MemoryCitation`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"entries\","]
    #[doc = "    \"threadIds\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"entries\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"$ref\": \"#/definitions/MemoryCitationEntry\""]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    \"threadIds\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"type\": \"string\""]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct MemoryCitation {
        pub entries: ::std::vec::Vec<MemoryCitationEntry>,
        #[serde(rename = "threadIds")]
        pub thread_ids: ::std::vec::Vec<::std::string::String>,
    }
    #[doc = "`MemoryCitationEntry`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"lineEnd\","]
    #[doc = "    \"lineStart\","]
    #[doc = "    \"note\","]
    #[doc = "    \"path\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"lineEnd\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"uint32\","]
    #[doc = "      \"minimum\": 0.0"]
    #[doc = "    },"]
    #[doc = "    \"lineStart\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"uint32\","]
    #[doc = "      \"minimum\": 0.0"]
    #[doc = "    },"]
    #[doc = "    \"note\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"path\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct MemoryCitationEntry {
        #[serde(rename = "lineEnd")]
        pub line_end: u32,
        #[serde(rename = "lineStart")]
        pub line_start: u32,
        pub note: ::std::string::String,
        pub path: ::std::string::String,
    }
    #[doc = "Classifies an assistant message as interim commentary or final answer text.\n\nProviders do not emit this consistently, so callers must treat `None` as \"phase unknown\" and keep compatibility behavior for legacy models."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"Classifies an assistant message as interim commentary or final answer text.\\n\\nProviders do not emit this consistently, so callers must treat `None` as \\\"phase unknown\\\" and keep compatibility behavior for legacy models.\","]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"description\": \"Mid-turn assistant text (for example preamble/progress narration).\\n\\nAdditional tool calls or assistant output may follow before turn completion.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"commentary\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"description\": \"The assistant's terminal answer text for the current turn.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"final_answer\""]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum MessagePhase {
        #[doc = "Mid-turn assistant text (for example preamble/progress narration).\n\nAdditional tool calls or assistant output may follow before turn completion."]
        #[serde(rename = "commentary")]
        Commentary,
        #[doc = "The assistant's terminal answer text for the current turn."]
        #[serde(rename = "final_answer")]
        FinalAnswer,
    }
    impl ::std::fmt::Display for MessagePhase {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Commentary => f.write_str("commentary"),
                Self::FinalAnswer => f.write_str("final_answer"),
            }
        }
    }
    impl ::std::str::FromStr for MessagePhase {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "commentary" => Ok(Self::Commentary),
                "final_answer" => Ok(Self::FinalAnswer),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for MessagePhase {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for MessagePhase {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for MessagePhase {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "Initial collaboration mode to use when the TUI starts."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"Initial collaboration mode to use when the TUI starts.\","]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"plan\","]
    #[doc = "    \"default\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum ModeKind {
        #[serde(rename = "plan")]
        Plan,
        #[serde(rename = "default")]
        Default,
    }
    impl ::std::fmt::Display for ModeKind {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Plan => f.write_str("plan"),
                Self::Default => f.write_str("default"),
            }
        }
    }
    impl ::std::str::FromStr for ModeKind {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "plan" => Ok(Self::Plan),
                "default" => Ok(Self::Default),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for ModeKind {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for ModeKind {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for ModeKind {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`ModelRerouteReason`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"highRiskCyberActivity\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum ModelRerouteReason {
        #[serde(rename = "highRiskCyberActivity")]
        HighRiskCyberActivity,
    }
    impl ::std::fmt::Display for ModelRerouteReason {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::HighRiskCyberActivity => f.write_str("highRiskCyberActivity"),
            }
        }
    }
    impl ::std::str::FromStr for ModelRerouteReason {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "highRiskCyberActivity" => Ok(Self::HighRiskCyberActivity),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for ModelRerouteReason {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for ModelRerouteReason {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for ModelRerouteReason {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`ModelReroutedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ModelReroutedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"fromModel\","]
    #[doc = "    \"reason\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"toModel\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"fromModel\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"reason\": {"]
    #[doc = "      \"$ref\": \"#/definitions/ModelRerouteReason\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"toModel\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ModelReroutedNotification {
        #[serde(rename = "fromModel")]
        pub from_model: ::std::string::String,
        pub reason: ModelRerouteReason,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "toModel")]
        pub to_model: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`ModelSafetyBufferingUpdatedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ModelSafetyBufferingUpdatedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"model\","]
    #[doc = "    \"reasons\","]
    #[doc = "    \"showBufferingUi\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\","]
    #[doc = "    \"useCases\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"fasterModel\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"model\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"reasons\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"type\": \"string\""]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    \"showBufferingUi\": {"]
    #[doc = "      \"type\": \"boolean\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"useCases\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"type\": \"string\""]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ModelSafetyBufferingUpdatedNotification {
        #[serde(
            rename = "fasterModel",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub faster_model: ::std::option::Option<::std::option::Option<::std::string::String>>,
        pub model: ::std::string::String,
        pub reasons: ::std::vec::Vec<::std::string::String>,
        #[serde(rename = "showBufferingUi")]
        pub show_buffering_ui: bool,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
        #[serde(rename = "useCases")]
        pub use_cases: ::std::vec::Vec<::std::string::String>,
    }
    #[doc = "`ModelVerification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"trustedAccessForCyber\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum ModelVerification {
        #[serde(rename = "trustedAccessForCyber")]
        TrustedAccessForCyber,
    }
    impl ::std::fmt::Display for ModelVerification {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::TrustedAccessForCyber => f.write_str("trustedAccessForCyber"),
            }
        }
    }
    impl ::std::str::FromStr for ModelVerification {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "trustedAccessForCyber" => Ok(Self::TrustedAccessForCyber),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for ModelVerification {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for ModelVerification {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for ModelVerification {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`ModelVerificationNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ModelVerificationNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\","]
    #[doc = "    \"verifications\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"verifications\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"$ref\": \"#/definitions/ModelVerification\""]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ModelVerificationNotification {
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
        pub verifications: ::std::vec::Vec<ModelVerification>,
    }
    #[doc = "`NetworkAccess`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"restricted\","]
    #[doc = "    \"enabled\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum NetworkAccess {
        #[serde(rename = "restricted")]
        Restricted,
        #[serde(rename = "enabled")]
        Enabled,
    }
    impl ::std::fmt::Display for NetworkAccess {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Restricted => f.write_str("restricted"),
                Self::Enabled => f.write_str("enabled"),
            }
        }
    }
    impl ::std::str::FromStr for NetworkAccess {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "restricted" => Ok(Self::Restricted),
                "enabled" => Ok(Self::Enabled),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for NetworkAccess {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for NetworkAccess {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for NetworkAccess {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`NetworkApprovalProtocol`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"http\","]
    #[doc = "    \"https\","]
    #[doc = "    \"socks5Tcp\","]
    #[doc = "    \"socks5Udp\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum NetworkApprovalProtocol {
        #[serde(rename = "http")]
        Http,
        #[serde(rename = "https")]
        Https,
        #[serde(rename = "socks5Tcp")]
        Socks5Tcp,
        #[serde(rename = "socks5Udp")]
        Socks5Udp,
    }
    impl ::std::fmt::Display for NetworkApprovalProtocol {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Http => f.write_str("http"),
                Self::Https => f.write_str("https"),
                Self::Socks5Tcp => f.write_str("socks5Tcp"),
                Self::Socks5Udp => f.write_str("socks5Udp"),
            }
        }
    }
    impl ::std::str::FromStr for NetworkApprovalProtocol {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "http" => Ok(Self::Http),
                "https" => Ok(Self::Https),
                "socks5Tcp" => Ok(Self::Socks5Tcp),
                "socks5Udp" => Ok(Self::Socks5Udp),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for NetworkApprovalProtocol {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for NetworkApprovalProtocol {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for NetworkApprovalProtocol {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`NonSteerableTurnKind`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"review\","]
    #[doc = "    \"compact\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum NonSteerableTurnKind {
        #[serde(rename = "review")]
        Review,
        #[serde(rename = "compact")]
        Compact,
    }
    impl ::std::fmt::Display for NonSteerableTurnKind {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Review => f.write_str("review"),
                Self::Compact => f.write_str("compact"),
            }
        }
    }
    impl ::std::str::FromStr for NonSteerableTurnKind {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "review" => Ok(Self::Review),
                "compact" => Ok(Self::Compact),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for NonSteerableTurnKind {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for NonSteerableTurnKind {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for NonSteerableTurnKind {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`PatchApplyStatus`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"inProgress\","]
    #[doc = "    \"completed\","]
    #[doc = "    \"failed\","]
    #[doc = "    \"declined\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum PatchApplyStatus {
        #[serde(rename = "inProgress")]
        InProgress,
        #[serde(rename = "completed")]
        Completed,
        #[serde(rename = "failed")]
        Failed,
        #[serde(rename = "declined")]
        Declined,
    }
    impl ::std::fmt::Display for PatchApplyStatus {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::InProgress => f.write_str("inProgress"),
                Self::Completed => f.write_str("completed"),
                Self::Failed => f.write_str("failed"),
                Self::Declined => f.write_str("declined"),
            }
        }
    }
    impl ::std::str::FromStr for PatchApplyStatus {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "inProgress" => Ok(Self::InProgress),
                "completed" => Ok(Self::Completed),
                "failed" => Ok(Self::Failed),
                "declined" => Ok(Self::Declined),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for PatchApplyStatus {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for PatchApplyStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for PatchApplyStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`PatchChangeKind`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"AddPatchChangeKind\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"AddPatchChangeKindType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"add\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"DeletePatchChangeKind\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"DeletePatchChangeKindType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"delete\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"UpdatePatchChangeKind\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"move_path\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"UpdatePatchChangeKindType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"update\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type")]
    pub enum PatchChangeKind {
        #[serde(rename = "add")]
        Add,
        #[serde(rename = "delete")]
        Delete,
        #[doc = "UpdatePatchChangeKind"]
        #[serde(rename = "update")]
        Update {
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            move_path: ::std::option::Option<::std::option::Option<::std::string::String>>,
        },
    }
    #[doc = "`Personality`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"none\","]
    #[doc = "    \"friendly\","]
    #[doc = "    \"pragmatic\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum Personality {
        #[serde(rename = "none")]
        None,
        #[serde(rename = "friendly")]
        Friendly,
        #[serde(rename = "pragmatic")]
        Pragmatic,
    }
    impl ::std::fmt::Display for Personality {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::None => f.write_str("none"),
                Self::Friendly => f.write_str("friendly"),
                Self::Pragmatic => f.write_str("pragmatic"),
            }
        }
    }
    impl ::std::str::FromStr for Personality {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "none" => Ok(Self::None),
                "friendly" => Ok(Self::Friendly),
                "pragmatic" => Ok(Self::Pragmatic),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for Personality {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for Personality {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for Personality {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "EXPERIMENTAL - proposed plan streaming deltas for plan items. Clients should not assume concatenated deltas match the completed plan item content."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"PlanDeltaNotification\","]
    #[doc = "  \"description\": \"EXPERIMENTAL - proposed plan streaming deltas for plan items. Clients should not assume concatenated deltas match the completed plan item content.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"delta\","]
    #[doc = "    \"itemId\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"delta\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"itemId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct PlanDeltaNotification {
        pub delta: ::std::string::String,
        #[serde(rename = "itemId")]
        pub item_id: ::std::string::String,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`PlanType`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"free\","]
    #[doc = "    \"go\","]
    #[doc = "    \"plus\","]
    #[doc = "    \"pro\","]
    #[doc = "    \"prolite\","]
    #[doc = "    \"team\","]
    #[doc = "    \"self_serve_business_usage_based\","]
    #[doc = "    \"business\","]
    #[doc = "    \"enterprise_cbp_usage_based\","]
    #[doc = "    \"enterprise\","]
    #[doc = "    \"edu\","]
    #[doc = "    \"unknown\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum PlanType {
        #[serde(rename = "free")]
        Free,
        #[serde(rename = "go")]
        Go,
        #[serde(rename = "plus")]
        Plus,
        #[serde(rename = "pro")]
        Pro,
        #[serde(rename = "prolite")]
        Prolite,
        #[serde(rename = "team")]
        Team,
        #[serde(rename = "self_serve_business_usage_based")]
        SelfServeBusinessUsageBased,
        #[serde(rename = "business")]
        Business,
        #[serde(rename = "enterprise_cbp_usage_based")]
        EnterpriseCbpUsageBased,
        #[serde(rename = "enterprise")]
        Enterprise,
        #[serde(rename = "edu")]
        Edu,
        #[serde(rename = "unknown")]
        Unknown,
    }
    impl ::std::fmt::Display for PlanType {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Free => f.write_str("free"),
                Self::Go => f.write_str("go"),
                Self::Plus => f.write_str("plus"),
                Self::Pro => f.write_str("pro"),
                Self::Prolite => f.write_str("prolite"),
                Self::Team => f.write_str("team"),
                Self::SelfServeBusinessUsageBased => f.write_str("self_serve_business_usage_based"),
                Self::Business => f.write_str("business"),
                Self::EnterpriseCbpUsageBased => f.write_str("enterprise_cbp_usage_based"),
                Self::Enterprise => f.write_str("enterprise"),
                Self::Edu => f.write_str("edu"),
                Self::Unknown => f.write_str("unknown"),
            }
        }
    }
    impl ::std::str::FromStr for PlanType {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "free" => Ok(Self::Free),
                "go" => Ok(Self::Go),
                "plus" => Ok(Self::Plus),
                "pro" => Ok(Self::Pro),
                "prolite" => Ok(Self::Prolite),
                "team" => Ok(Self::Team),
                "self_serve_business_usage_based" => Ok(Self::SelfServeBusinessUsageBased),
                "business" => Ok(Self::Business),
                "enterprise_cbp_usage_based" => Ok(Self::EnterpriseCbpUsageBased),
                "enterprise" => Ok(Self::Enterprise),
                "edu" => Ok(Self::Edu),
                "unknown" => Ok(Self::Unknown),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for PlanType {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for PlanType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for PlanType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "Final process exit notification for `process/spawn`."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ProcessExitedNotification\","]
    #[doc = "  \"description\": \"Final process exit notification for `process/spawn`.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"exitCode\","]
    #[doc = "    \"processHandle\","]
    #[doc = "    \"stderr\","]
    #[doc = "    \"stderrCapReached\","]
    #[doc = "    \"stdout\","]
    #[doc = "    \"stdoutCapReached\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"exitCode\": {"]
    #[doc = "      \"description\": \"Process exit code.\","]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int32\""]
    #[doc = "    },"]
    #[doc = "    \"processHandle\": {"]
    #[doc = "      \"description\": \"Client-supplied, connection-scoped `processHandle` from `process/spawn`.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"stderr\": {"]
    #[doc = "      \"description\": \"Buffered stderr capture.\\n\\nEmpty when stderr was streamed via `process/outputDelta`.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"stderrCapReached\": {"]
    #[doc = "      \"description\": \"Whether stderr reached `outputBytesCap`.\\n\\nIn streaming mode, stderr is empty and cap state is also reported on the final stderr `process/outputDelta` notification.\","]
    #[doc = "      \"type\": \"boolean\""]
    #[doc = "    },"]
    #[doc = "    \"stdout\": {"]
    #[doc = "      \"description\": \"Buffered stdout capture.\\n\\nEmpty when stdout was streamed via `process/outputDelta`.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"stdoutCapReached\": {"]
    #[doc = "      \"description\": \"Whether stdout reached `outputBytesCap`.\\n\\nIn streaming mode, stdout is empty and cap state is also reported on the final stdout `process/outputDelta` notification.\","]
    #[doc = "      \"type\": \"boolean\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ProcessExitedNotification {
        #[doc = "Process exit code."]
        #[serde(rename = "exitCode")]
        pub exit_code: i32,
        #[doc = "Client-supplied, connection-scoped `processHandle` from `process/spawn`."]
        #[serde(rename = "processHandle")]
        pub process_handle: ::std::string::String,
        #[doc = "Buffered stderr capture.\n\nEmpty when stderr was streamed via `process/outputDelta`."]
        pub stderr: ::std::string::String,
        #[doc = "Whether stderr reached `outputBytesCap`.\n\nIn streaming mode, stderr is empty and cap state is also reported on the final stderr `process/outputDelta` notification."]
        #[serde(rename = "stderrCapReached")]
        pub stderr_cap_reached: bool,
        #[doc = "Buffered stdout capture.\n\nEmpty when stdout was streamed via `process/outputDelta`."]
        pub stdout: ::std::string::String,
        #[doc = "Whether stdout reached `outputBytesCap`.\n\nIn streaming mode, stdout is empty and cap state is also reported on the final stdout `process/outputDelta` notification."]
        #[serde(rename = "stdoutCapReached")]
        pub stdout_cap_reached: bool,
    }
    #[doc = "Base64-encoded output chunk emitted for a streaming `process/spawn` request."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ProcessOutputDeltaNotification\","]
    #[doc = "  \"description\": \"Base64-encoded output chunk emitted for a streaming `process/spawn` request.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"capReached\","]
    #[doc = "    \"deltaBase64\","]
    #[doc = "    \"processHandle\","]
    #[doc = "    \"stream\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"capReached\": {"]
    #[doc = "      \"description\": \"True on the final streamed chunk for this stream when output was truncated by `outputBytesCap`.\","]
    #[doc = "      \"type\": \"boolean\""]
    #[doc = "    },"]
    #[doc = "    \"deltaBase64\": {"]
    #[doc = "      \"description\": \"Base64-encoded output bytes.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"processHandle\": {"]
    #[doc = "      \"description\": \"Client-supplied, connection-scoped `processHandle` from `process/spawn`.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"stream\": {"]
    #[doc = "      \"description\": \"Output stream this chunk belongs to.\","]
    #[doc = "      \"allOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/ProcessOutputStream\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ProcessOutputDeltaNotification {
        #[doc = "True on the final streamed chunk for this stream when output was truncated by `outputBytesCap`."]
        #[serde(rename = "capReached")]
        pub cap_reached: bool,
        #[doc = "Base64-encoded output bytes."]
        #[serde(rename = "deltaBase64")]
        pub delta_base64: ::std::string::String,
        #[doc = "Client-supplied, connection-scoped `processHandle` from `process/spawn`."]
        #[serde(rename = "processHandle")]
        pub process_handle: ::std::string::String,
        #[doc = "Output stream this chunk belongs to."]
        pub stream: ProcessOutputStream,
    }
    #[doc = "Stream label for `process/outputDelta` notifications."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"Stream label for `process/outputDelta` notifications.\","]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"description\": \"stdout stream. PTY mode multiplexes terminal output here.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"stdout\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"description\": \"stderr stream.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"stderr\""]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum ProcessOutputStream {
        #[doc = "stdout stream. PTY mode multiplexes terminal output here."]
        #[serde(rename = "stdout")]
        Stdout,
        #[doc = "stderr stream."]
        #[serde(rename = "stderr")]
        Stderr,
    }
    impl ::std::fmt::Display for ProcessOutputStream {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Stdout => f.write_str("stdout"),
                Self::Stderr => f.write_str("stderr"),
            }
        }
    }
    impl ::std::str::FromStr for ProcessOutputStream {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "stdout" => Ok(Self::Stdout),
                "stderr" => Ok(Self::Stderr),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for ProcessOutputStream {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for ProcessOutputStream {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for ProcessOutputStream {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`RateLimitReachedType`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"rate_limit_reached\","]
    #[doc = "    \"workspace_owner_credits_depleted\","]
    #[doc = "    \"workspace_member_credits_depleted\","]
    #[doc = "    \"workspace_owner_usage_limit_reached\","]
    #[doc = "    \"workspace_member_usage_limit_reached\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum RateLimitReachedType {
        #[serde(rename = "rate_limit_reached")]
        RateLimitReached,
        #[serde(rename = "workspace_owner_credits_depleted")]
        WorkspaceOwnerCreditsDepleted,
        #[serde(rename = "workspace_member_credits_depleted")]
        WorkspaceMemberCreditsDepleted,
        #[serde(rename = "workspace_owner_usage_limit_reached")]
        WorkspaceOwnerUsageLimitReached,
        #[serde(rename = "workspace_member_usage_limit_reached")]
        WorkspaceMemberUsageLimitReached,
    }
    impl ::std::fmt::Display for RateLimitReachedType {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::RateLimitReached => f.write_str("rate_limit_reached"),
                Self::WorkspaceOwnerCreditsDepleted => {
                    f.write_str("workspace_owner_credits_depleted")
                }
                Self::WorkspaceMemberCreditsDepleted => {
                    f.write_str("workspace_member_credits_depleted")
                }
                Self::WorkspaceOwnerUsageLimitReached => {
                    f.write_str("workspace_owner_usage_limit_reached")
                }
                Self::WorkspaceMemberUsageLimitReached => {
                    f.write_str("workspace_member_usage_limit_reached")
                }
            }
        }
    }
    impl ::std::str::FromStr for RateLimitReachedType {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "rate_limit_reached" => Ok(Self::RateLimitReached),
                "workspace_owner_credits_depleted" => Ok(Self::WorkspaceOwnerCreditsDepleted),
                "workspace_member_credits_depleted" => Ok(Self::WorkspaceMemberCreditsDepleted),
                "workspace_owner_usage_limit_reached" => Ok(Self::WorkspaceOwnerUsageLimitReached),
                "workspace_member_usage_limit_reached" => {
                    Ok(Self::WorkspaceMemberUsageLimitReached)
                }
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for RateLimitReachedType {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for RateLimitReachedType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for RateLimitReachedType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`RateLimitSnapshot`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"credits\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/CreditsSnapshot\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"individualLimit\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/SpendControlLimitSnapshot\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"limitId\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"limitName\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"planType\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/PlanType\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"primary\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/RateLimitWindow\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"rateLimitReachedType\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/RateLimitReachedType\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"secondary\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/RateLimitWindow\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct RateLimitSnapshot {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub credits: ::std::option::Option<CreditsSnapshot>,
        #[serde(
            rename = "individualLimit",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub individual_limit: ::std::option::Option<SpendControlLimitSnapshot>,
        #[serde(
            rename = "limitId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub limit_id: ::std::option::Option<::std::string::String>,
        #[serde(
            rename = "limitName",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub limit_name: ::std::option::Option<::std::string::String>,
        #[serde(
            rename = "planType",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub plan_type: ::std::option::Option<PlanType>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub primary: ::std::option::Option<RateLimitWindow>,
        #[serde(
            rename = "rateLimitReachedType",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub rate_limit_reached_type: ::std::option::Option<RateLimitReachedType>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub secondary: ::std::option::Option<RateLimitWindow>,
    }
    impl ::std::default::Default for RateLimitSnapshot {
        fn default() -> Self {
            Self {
                credits: Default::default(),
                individual_limit: Default::default(),
                limit_id: Default::default(),
                limit_name: Default::default(),
                plan_type: Default::default(),
                primary: Default::default(),
                rate_limit_reached_type: Default::default(),
                secondary: Default::default(),
            }
        }
    }
    #[doc = "`RateLimitWindow`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"usedPercent\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"resetsAt\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"int64\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"usedPercent\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int32\""]
    #[doc = "    },"]
    #[doc = "    \"windowDurationMins\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"int64\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct RateLimitWindow {
        #[serde(
            rename = "resetsAt",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[ts(type = "number | null")]
        pub resets_at: ::std::option::Option<i64>,
        #[serde(rename = "usedPercent")]
        pub used_percent: i32,
        #[serde(
            rename = "windowDurationMins",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[ts(type = "number | null")]
        pub window_duration_mins: ::std::option::Option<i64>,
    }
    #[doc = "`RealtimeConversationVersion`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"v1\","]
    #[doc = "    \"v2\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum RealtimeConversationVersion {
        #[serde(rename = "v1")]
        V1,
        #[serde(rename = "v2")]
        V2,
    }
    impl ::std::fmt::Display for RealtimeConversationVersion {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::V1 => f.write_str("v1"),
                Self::V2 => f.write_str("v2"),
            }
        }
    }
    impl ::std::str::FromStr for RealtimeConversationVersion {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "v1" => Ok(Self::V1),
                "v2" => Ok(Self::V2),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for RealtimeConversationVersion {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for RealtimeConversationVersion {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for RealtimeConversationVersion {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "A non-empty reasoning effort value advertised by the model."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"A non-empty reasoning effort value advertised by the model.\","]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"minLength\": 1"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    #[serde(transparent)]
    pub struct ReasoningEffort(::std::string::String);
    impl ::std::ops::Deref for ReasoningEffort {
        type Target = ::std::string::String;
        fn deref(&self) -> &::std::string::String {
            &self.0
        }
    }
    impl ::std::convert::From<ReasoningEffort> for ::std::string::String {
        fn from(value: ReasoningEffort) -> Self {
            value.0
        }
    }
    impl ::std::str::FromStr for ReasoningEffort {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            if value.chars().count() < 1usize {
                return Err("shorter than 1 characters".into());
            }
            Ok(Self(value.to_string()))
        }
    }
    impl ::std::convert::TryFrom<&str> for ReasoningEffort {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for ReasoningEffort {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for ReasoningEffort {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl<'de> ::serde::Deserialize<'de> for ReasoningEffort {
        fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
        where
            D: ::serde::Deserializer<'de>,
        {
            ::std::string::String::deserialize(deserializer)?
                .parse()
                .map_err(|e: self::error::ConversionError| {
                    <D::Error as ::serde::de::Error>::custom(e.to_string())
                })
        }
    }
    #[doc = "A summary of the reasoning performed by the model. This can be useful for debugging and understanding the model's reasoning process. See https://platform.openai.com/docs/guides/reasoning?api-mode=responses#reasoning-summaries"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"A summary of the reasoning performed by the model. This can be useful for debugging and understanding the model's reasoning process. See https://platform.openai.com/docs/guides/reasoning?api-mode=responses#reasoning-summaries\","]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"auto\","]
    #[doc = "        \"concise\","]
    #[doc = "        \"detailed\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"description\": \"Option to disable reasoning summaries.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"none\""]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum ReasoningSummary {
        #[serde(rename = "auto")]
        Auto,
        #[serde(rename = "concise")]
        Concise,
        #[serde(rename = "detailed")]
        Detailed,
        #[doc = "Option to disable reasoning summaries."]
        #[serde(rename = "none")]
        None,
    }
    impl ::std::fmt::Display for ReasoningSummary {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Auto => f.write_str("auto"),
                Self::Concise => f.write_str("concise"),
                Self::Detailed => f.write_str("detailed"),
                Self::None => f.write_str("none"),
            }
        }
    }
    impl ::std::str::FromStr for ReasoningSummary {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "auto" => Ok(Self::Auto),
                "concise" => Ok(Self::Concise),
                "detailed" => Ok(Self::Detailed),
                "none" => Ok(Self::None),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for ReasoningSummary {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for ReasoningSummary {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for ReasoningSummary {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`ReasoningSummaryPartAddedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ReasoningSummaryPartAddedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"itemId\","]
    #[doc = "    \"summaryIndex\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"itemId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"summaryIndex\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ReasoningSummaryPartAddedNotification {
        #[serde(rename = "itemId")]
        pub item_id: ::std::string::String,
        #[serde(rename = "summaryIndex")]
        #[ts(type = "number")]
        pub summary_index: i64,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`ReasoningSummaryTextDeltaNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ReasoningSummaryTextDeltaNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"delta\","]
    #[doc = "    \"itemId\","]
    #[doc = "    \"summaryIndex\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"delta\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"itemId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"summaryIndex\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ReasoningSummaryTextDeltaNotification {
        pub delta: ::std::string::String,
        #[serde(rename = "itemId")]
        pub item_id: ::std::string::String,
        #[serde(rename = "summaryIndex")]
        #[ts(type = "number")]
        pub summary_index: i64,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`ReasoningTextDeltaNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ReasoningTextDeltaNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"contentIndex\","]
    #[doc = "    \"delta\","]
    #[doc = "    \"itemId\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"contentIndex\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"delta\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"itemId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ReasoningTextDeltaNotification {
        #[serde(rename = "contentIndex")]
        #[ts(type = "number")]
        pub content_index: i64,
        pub delta: ::std::string::String,
        #[serde(rename = "itemId")]
        pub item_id: ::std::string::String,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`RemoteControlConnectionStatus`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"disabled\","]
    #[doc = "    \"connecting\","]
    #[doc = "    \"connected\","]
    #[doc = "    \"errored\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum RemoteControlConnectionStatus {
        #[serde(rename = "disabled")]
        Disabled,
        #[serde(rename = "connecting")]
        Connecting,
        #[serde(rename = "connected")]
        Connected,
        #[serde(rename = "errored")]
        Errored,
    }
    impl ::std::fmt::Display for RemoteControlConnectionStatus {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Disabled => f.write_str("disabled"),
                Self::Connecting => f.write_str("connecting"),
                Self::Connected => f.write_str("connected"),
                Self::Errored => f.write_str("errored"),
            }
        }
    }
    impl ::std::str::FromStr for RemoteControlConnectionStatus {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "disabled" => Ok(Self::Disabled),
                "connecting" => Ok(Self::Connecting),
                "connected" => Ok(Self::Connected),
                "errored" => Ok(Self::Errored),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for RemoteControlConnectionStatus {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for RemoteControlConnectionStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for RemoteControlConnectionStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "Current remote-control connection status and remote identity exposed to clients."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"RemoteControlStatusChangedNotification\","]
    #[doc = "  \"description\": \"Current remote-control connection status and remote identity exposed to clients.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"installationId\","]
    #[doc = "    \"serverName\","]
    #[doc = "    \"status\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"environmentId\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"installationId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"serverName\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"status\": {"]
    #[doc = "      \"$ref\": \"#/definitions/RemoteControlConnectionStatus\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct RemoteControlStatusChangedNotification {
        #[serde(
            rename = "environmentId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub environment_id: ::std::option::Option<::std::string::String>,
        #[serde(rename = "installationId")]
        pub installation_id: ::std::string::String,
        #[serde(rename = "serverName")]
        pub server_name: ::std::string::String,
        pub status: RemoteControlConnectionStatus,
    }
    #[doc = "`RequestId`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"anyOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(untagged)]
    pub enum RequestId {
        String(::std::string::String),
        Int64(#[ts(type = "number")] i64),
    }
    impl ::std::fmt::Display for RequestId {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match self {
                Self::String(x) => x.fmt(f),
                Self::Int64(x) => x.fmt(f),
            }
        }
    }
    impl ::std::convert::From<i64> for RequestId {
        fn from(value: i64) -> Self {
            Self::Int64(value)
        }
    }
    #[doc = "`RequestPermissionProfile`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"fileSystem\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/AdditionalFileSystemPermissions\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"network\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/AdditionalNetworkPermissions\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"additionalProperties\": false"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(deny_unknown_fields)]
    #[ts(rename = "ServerNotificationRequestPermissionProfile")]
    pub struct RequestPermissionProfile {
        #[serde(
            rename = "fileSystem",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub file_system:
            ::std::option::Option<::std::option::Option<AdditionalFileSystemPermissions>>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub network: ::std::option::Option<::std::option::Option<AdditionalNetworkPermissions>>,
    }
    impl ::std::default::Default for RequestPermissionProfile {
        fn default() -> Self {
            Self {
                file_system: Default::default(),
                network: Default::default(),
            }
        }
    }
    #[doc = "`SandboxPolicy`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"DangerFullAccessSandboxPolicy\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"DangerFullAccessSandboxPolicyType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"dangerFullAccess\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ReadOnlySandboxPolicy\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"networkAccess\": {"]
    #[doc = "          \"default\": false,"]
    #[doc = "          \"type\": \"boolean\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ReadOnlySandboxPolicyType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"readOnly\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ExternalSandboxSandboxPolicy\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"networkAccess\": {"]
    #[doc = "          \"default\": \"restricted\","]
    #[doc = "          \"allOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/NetworkAccess\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ExternalSandboxSandboxPolicyType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"externalSandbox\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"WorkspaceWriteSandboxPolicy\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"excludeSlashTmp\": {"]
    #[doc = "          \"default\": false,"]
    #[doc = "          \"type\": \"boolean\""]
    #[doc = "        },"]
    #[doc = "        \"excludeTmpdirEnvVar\": {"]
    #[doc = "          \"default\": false,"]
    #[doc = "          \"type\": \"boolean\""]
    #[doc = "        },"]
    #[doc = "        \"networkAccess\": {"]
    #[doc = "          \"default\": false,"]
    #[doc = "          \"type\": \"boolean\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"WorkspaceWriteSandboxPolicyType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"workspaceWrite\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"writableRoots\": {"]
    #[doc = "          \"default\": [],"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/AbsolutePathBuf\""]
    #[doc = "          }"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type")]
    pub enum SandboxPolicy {
        #[serde(rename = "dangerFullAccess")]
        DangerFullAccess,
        #[doc = "ReadOnlySandboxPolicy"]
        #[serde(rename = "readOnly")]
        ReadOnly {
            #[serde(rename = "networkAccess", default)]
            network_access: bool,
        },
        #[doc = "ExternalSandboxSandboxPolicy"]
        #[serde(rename = "externalSandbox")]
        ExternalSandbox {
            #[serde(
                rename = "networkAccess",
                default = "defaults::sandbox_policy_external_sandbox_network_access"
            )]
            network_access: NetworkAccess,
        },
        #[doc = "WorkspaceWriteSandboxPolicy"]
        #[serde(rename = "workspaceWrite")]
        WorkspaceWrite {
            #[serde(rename = "excludeSlashTmp", default)]
            exclude_slash_tmp: bool,
            #[serde(rename = "excludeTmpdirEnvVar", default)]
            exclude_tmpdir_env_var: bool,
            #[serde(rename = "networkAccess", default)]
            network_access: bool,
            #[serde(rename = "writableRoots", default)]
            writable_roots: ::std::vec::Vec<AbsolutePathBuf>,
        },
    }
    #[doc = "Notification sent from the server to the client."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ServerNotification\","]
    #[doc = "  \"description\": \"Notification sent from the server to the client.\","]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ErrorNotification\","]
    #[doc = "      \"description\": \"NEW NOTIFICATIONS\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"ErrorNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"error\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ErrorNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/startedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/startedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/started\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadStartedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/status/changedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/status/changedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/status/changed\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadStatusChangedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/archivedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/archivedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/archived\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadArchivedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/deletedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/deletedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/deleted\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadDeletedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/unarchivedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/unarchivedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/unarchived\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadUnarchivedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/closedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/closedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/closed\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadClosedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Skills/changedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Skills/changedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"skills/changed\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/SkillsChangedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/name/updatedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/name/updatedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/name/updated\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadNameUpdatedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/goal/updatedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/goal/updatedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/goal/updated\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadGoalUpdatedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/goal/clearedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/goal/clearedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/goal/cleared\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadGoalClearedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/settings/updatedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/settings/updatedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/settings/updated\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadSettingsUpdatedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/tokenUsage/updatedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/tokenUsage/updatedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/tokenUsage/updated\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadTokenUsageUpdatedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Turn/startedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Turn/startedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"turn/started\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/TurnStartedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Hook/startedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Hook/startedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"hook/started\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/HookStartedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Turn/completedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Turn/completedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"turn/completed\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/TurnCompletedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Hook/completedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Hook/completedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"hook/completed\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/HookCompletedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Turn/diff/updatedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Turn/diff/updatedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"turn/diff/updated\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/TurnDiffUpdatedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Turn/plan/updatedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Turn/plan/updatedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"turn/plan/updated\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/TurnPlanUpdatedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Item/startedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Item/startedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"item/started\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ItemStartedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Item/autoApprovalReview/startedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Item/autoApprovalReview/startedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"item/autoApprovalReview/started\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ItemGuardianApprovalReviewStartedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Item/autoApprovalReview/completedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Item/autoApprovalReview/completedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"item/autoApprovalReview/completed\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ItemGuardianApprovalReviewCompletedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Item/completedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Item/completedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"item/completed\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ItemCompletedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Item/agentMessage/deltaNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Item/agentMessage/deltaNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"item/agentMessage/delta\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/AgentMessageDeltaNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Item/plan/deltaNotification\","]
    #[doc = "      \"description\": \"EXPERIMENTAL - proposed plan streaming deltas for plan items.\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Item/plan/deltaNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"item/plan/delta\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/PlanDeltaNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Command/exec/outputDeltaNotification\","]
    #[doc = "      \"description\": \"Stream base64-encoded stdout/stderr chunks for a running `command/exec` session.\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Command/exec/outputDeltaNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"command/exec/outputDelta\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/CommandExecOutputDeltaNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Process/outputDeltaNotification\","]
    #[doc = "      \"description\": \"Stream base64-encoded stdout/stderr chunks for a running `process/spawn` session.\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Process/outputDeltaNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"process/outputDelta\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ProcessOutputDeltaNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Process/exitedNotification\","]
    #[doc = "      \"description\": \"Final exit notification for a `process/spawn` session.\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Process/exitedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"process/exited\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ProcessExitedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Item/commandExecution/outputDeltaNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Item/commandExecution/outputDeltaNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"item/commandExecution/outputDelta\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/CommandExecutionOutputDeltaNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Item/commandExecution/terminalInteractionNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Item/commandExecution/terminalInteractionNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"item/commandExecution/terminalInteraction\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/TerminalInteractionNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Item/fileChange/outputDeltaNotification\","]
    #[doc = "      \"description\": \"Deprecated legacy apply_patch output stream notification.\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Item/fileChange/outputDeltaNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"item/fileChange/outputDelta\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/FileChangeOutputDeltaNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Item/fileChange/patchUpdatedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Item/fileChange/patchUpdatedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"item/fileChange/patchUpdated\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/FileChangePatchUpdatedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ServerRequest/resolvedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"ServerRequest/resolvedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"serverRequest/resolved\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ServerRequestResolvedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Item/mcpToolCall/progressNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Item/mcpToolCall/progressNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"item/mcpToolCall/progress\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/McpToolCallProgressNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"McpServer/oauthLogin/completedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"McpServer/oauthLogin/completedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"mcpServer/oauthLogin/completed\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/McpServerOauthLoginCompletedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"McpServer/startupStatus/updatedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"McpServer/startupStatus/updatedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"mcpServer/startupStatus/updated\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/McpServerStatusUpdatedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Account/updatedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Account/updatedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"account/updated\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/AccountUpdatedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Account/rateLimits/updatedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Account/rateLimits/updatedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"account/rateLimits/updated\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/AccountRateLimitsUpdatedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"App/list/updatedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"App/list/updatedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"app/list/updated\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/AppListUpdatedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"RemoteControl/status/changedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"RemoteControl/status/changedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"remoteControl/status/changed\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/RemoteControlStatusChangedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ExternalAgentConfig/import/progressNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"ExternalAgentConfig/import/progressNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"externalAgentConfig/import/progress\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ExternalAgentConfigImportProgressNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ExternalAgentConfig/import/completedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"ExternalAgentConfig/import/completedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"externalAgentConfig/import/completed\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ExternalAgentConfigImportCompletedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Fs/changedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Fs/changedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"fs/changed\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/FsChangedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Item/reasoning/summaryTextDeltaNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Item/reasoning/summaryTextDeltaNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"item/reasoning/summaryTextDelta\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ReasoningSummaryTextDeltaNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Item/reasoning/summaryPartAddedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Item/reasoning/summaryPartAddedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"item/reasoning/summaryPartAdded\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ReasoningSummaryPartAddedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Item/reasoning/textDeltaNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Item/reasoning/textDeltaNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"item/reasoning/textDelta\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ReasoningTextDeltaNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/compactedNotification\","]
    #[doc = "      \"description\": \"Deprecated: Use `ContextCompaction` item type instead.\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/compactedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/compacted\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ContextCompactedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Model/reroutedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Model/reroutedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"model/rerouted\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ModelReroutedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Model/verificationNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Model/verificationNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"model/verification\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ModelVerificationNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Turn/moderationMetadataNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Turn/moderationMetadataNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"turn/moderationMetadata\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/TurnModerationMetadataNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Model/safetyBuffering/updatedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Model/safetyBuffering/updatedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"model/safetyBuffering/updated\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ModelSafetyBufferingUpdatedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"WarningNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"WarningNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"warning\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/WarningNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"GuardianWarningNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"GuardianWarningNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"guardianWarning\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/GuardianWarningNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"DeprecationNoticeNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"DeprecationNoticeNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"deprecationNotice\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/DeprecationNoticeNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ConfigWarningNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"ConfigWarningNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"configWarning\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ConfigWarningNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"FuzzyFileSearch/sessionUpdatedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"FuzzyFileSearch/sessionUpdatedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"fuzzyFileSearch/sessionUpdated\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/FuzzyFileSearchSessionUpdatedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"FuzzyFileSearch/sessionCompletedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"FuzzyFileSearch/sessionCompletedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"fuzzyFileSearch/sessionCompleted\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/FuzzyFileSearchSessionCompletedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/realtime/startedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/realtime/startedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/realtime/started\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadRealtimeStartedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/realtime/itemAddedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/realtime/itemAddedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/realtime/itemAdded\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadRealtimeItemAddedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/realtime/transcript/deltaNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/realtime/transcript/deltaNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/realtime/transcript/delta\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadRealtimeTranscriptDeltaNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/realtime/transcript/doneNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/realtime/transcript/doneNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/realtime/transcript/done\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadRealtimeTranscriptDoneNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/realtime/outputAudio/deltaNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/realtime/outputAudio/deltaNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/realtime/outputAudio/delta\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadRealtimeOutputAudioDeltaNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/realtime/sdpNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/realtime/sdpNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/realtime/sdp\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadRealtimeSdpNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/realtime/errorNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/realtime/errorNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/realtime/error\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadRealtimeErrorNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Thread/realtime/closedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Thread/realtime/closedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"thread/realtime/closed\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadRealtimeClosedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Windows/worldWritableWarningNotification\","]
    #[doc = "      \"description\": \"Notifies the user of world-writable directories on Windows, which cannot be protected by the sandbox.\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Windows/worldWritableWarningNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"windows/worldWritableWarning\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/WindowsWorldWritableWarningNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"WindowsSandbox/setupCompletedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"WindowsSandbox/setupCompletedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"windowsSandbox/setupCompleted\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/WindowsSandboxSetupCompletedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"Account/login/completedNotification\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"method\","]
    #[doc = "        \"params\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"method\": {"]
    #[doc = "          \"title\": \"Account/login/completedNotificationMethod\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"account/login/completed\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"params\": {"]
    #[doc = "          \"$ref\": \"#/definitions/AccountLoginCompletedNotification\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ],"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "method", content = "params")]
    pub enum ServerNotification {
        #[doc = "ErrorNotification\n\nNEW NOTIFICATIONS"]
        #[serde(rename = "error")]
        Error(ErrorNotification),
        #[doc = "Thread/startedNotification"]
        #[serde(rename = "thread/started")]
        ThreadStarted(ThreadStartedNotification),
        #[doc = "Thread/status/changedNotification"]
        #[serde(rename = "thread/status/changed")]
        ThreadStatusChanged(ThreadStatusChangedNotification),
        #[doc = "Thread/archivedNotification"]
        #[serde(rename = "thread/archived")]
        ThreadArchived(ThreadArchivedNotification),
        #[doc = "Thread/deletedNotification"]
        #[serde(rename = "thread/deleted")]
        ThreadDeleted(ThreadDeletedNotification),
        #[doc = "Thread/unarchivedNotification"]
        #[serde(rename = "thread/unarchived")]
        ThreadUnarchived(ThreadUnarchivedNotification),
        #[doc = "Thread/closedNotification"]
        #[serde(rename = "thread/closed")]
        ThreadClosed(ThreadClosedNotification),
        #[doc = "Skills/changedNotification"]
        #[serde(rename = "skills/changed")]
        SkillsChanged(SkillsChangedNotification),
        #[doc = "Thread/name/updatedNotification"]
        #[serde(rename = "thread/name/updated")]
        ThreadNameUpdated(ThreadNameUpdatedNotification),
        #[doc = "Thread/goal/updatedNotification"]
        #[serde(rename = "thread/goal/updated")]
        ThreadGoalUpdated(ThreadGoalUpdatedNotification),
        #[doc = "Thread/goal/clearedNotification"]
        #[serde(rename = "thread/goal/cleared")]
        ThreadGoalCleared(ThreadGoalClearedNotification),
        #[doc = "Thread/settings/updatedNotification"]
        #[serde(rename = "thread/settings/updated")]
        ThreadSettingsUpdated(ThreadSettingsUpdatedNotification),
        #[doc = "Thread/tokenUsage/updatedNotification"]
        #[serde(rename = "thread/tokenUsage/updated")]
        ThreadTokenUsageUpdated(ThreadTokenUsageUpdatedNotification),
        #[doc = "Turn/startedNotification"]
        #[serde(rename = "turn/started")]
        TurnStarted(TurnStartedNotification),
        #[doc = "Hook/startedNotification"]
        #[serde(rename = "hook/started")]
        HookStarted(HookStartedNotification),
        #[doc = "Turn/completedNotification"]
        #[serde(rename = "turn/completed")]
        TurnCompleted(TurnCompletedNotification),
        #[doc = "Hook/completedNotification"]
        #[serde(rename = "hook/completed")]
        HookCompleted(HookCompletedNotification),
        #[doc = "Turn/diff/updatedNotification"]
        #[serde(rename = "turn/diff/updated")]
        TurnDiffUpdated(TurnDiffUpdatedNotification),
        #[doc = "Turn/plan/updatedNotification"]
        #[serde(rename = "turn/plan/updated")]
        TurnPlanUpdated(TurnPlanUpdatedNotification),
        #[doc = "Item/startedNotification"]
        #[serde(rename = "item/started")]
        ItemStarted(ItemStartedNotification),
        #[doc = "Item/autoApprovalReview/startedNotification"]
        #[serde(rename = "item/autoApprovalReview/started")]
        ItemAutoApprovalReviewStarted(ItemGuardianApprovalReviewStartedNotification),
        #[doc = "Item/autoApprovalReview/completedNotification"]
        #[serde(rename = "item/autoApprovalReview/completed")]
        ItemAutoApprovalReviewCompleted(ItemGuardianApprovalReviewCompletedNotification),
        #[doc = "Item/completedNotification"]
        #[serde(rename = "item/completed")]
        ItemCompleted(ItemCompletedNotification),
        #[doc = "Item/agentMessage/deltaNotification"]
        #[serde(rename = "item/agentMessage/delta")]
        ItemAgentMessageDelta(AgentMessageDeltaNotification),
        #[doc = "Item/plan/deltaNotification\n\nEXPERIMENTAL - proposed plan streaming deltas for plan items."]
        #[serde(rename = "item/plan/delta")]
        ItemPlanDelta(PlanDeltaNotification),
        #[doc = "Command/exec/outputDeltaNotification\n\nStream base64-encoded stdout/stderr chunks for a running `command/exec` session."]
        #[serde(rename = "command/exec/outputDelta")]
        CommandExecOutputDelta(CommandExecOutputDeltaNotification),
        #[doc = "Process/outputDeltaNotification\n\nStream base64-encoded stdout/stderr chunks for a running `process/spawn` session."]
        #[serde(rename = "process/outputDelta")]
        ProcessOutputDelta(ProcessOutputDeltaNotification),
        #[doc = "Process/exitedNotification\n\nFinal exit notification for a `process/spawn` session."]
        #[serde(rename = "process/exited")]
        ProcessExited(ProcessExitedNotification),
        #[doc = "Item/commandExecution/outputDeltaNotification"]
        #[serde(rename = "item/commandExecution/outputDelta")]
        ItemCommandExecutionOutputDelta(CommandExecutionOutputDeltaNotification),
        #[doc = "Item/commandExecution/terminalInteractionNotification"]
        #[serde(rename = "item/commandExecution/terminalInteraction")]
        ItemCommandExecutionTerminalInteraction(TerminalInteractionNotification),
        #[doc = "Item/fileChange/outputDeltaNotification\n\nDeprecated legacy apply_patch output stream notification."]
        #[serde(rename = "item/fileChange/outputDelta")]
        ItemFileChangeOutputDelta(FileChangeOutputDeltaNotification),
        #[doc = "Item/fileChange/patchUpdatedNotification"]
        #[serde(rename = "item/fileChange/patchUpdated")]
        ItemFileChangePatchUpdated(FileChangePatchUpdatedNotification),
        #[doc = "ServerRequest/resolvedNotification"]
        #[serde(rename = "serverRequest/resolved")]
        ServerRequestResolved(ServerRequestResolvedNotification),
        #[doc = "Item/mcpToolCall/progressNotification"]
        #[serde(rename = "item/mcpToolCall/progress")]
        ItemMcpToolCallProgress(McpToolCallProgressNotification),
        #[doc = "McpServer/oauthLogin/completedNotification"]
        #[serde(rename = "mcpServer/oauthLogin/completed")]
        McpServerOauthLoginCompleted(McpServerOauthLoginCompletedNotification),
        #[doc = "McpServer/startupStatus/updatedNotification"]
        #[serde(rename = "mcpServer/startupStatus/updated")]
        McpServerStartupStatusUpdated(McpServerStatusUpdatedNotification),
        #[doc = "Account/updatedNotification"]
        #[serde(rename = "account/updated")]
        AccountUpdated(AccountUpdatedNotification),
        #[doc = "Account/rateLimits/updatedNotification"]
        #[serde(rename = "account/rateLimits/updated")]
        AccountRateLimitsUpdated(AccountRateLimitsUpdatedNotification),
        #[doc = "App/list/updatedNotification"]
        #[serde(rename = "app/list/updated")]
        AppListUpdated(AppListUpdatedNotification),
        #[doc = "RemoteControl/status/changedNotification"]
        #[serde(rename = "remoteControl/status/changed")]
        RemoteControlStatusChanged(RemoteControlStatusChangedNotification),
        #[doc = "ExternalAgentConfig/import/progressNotification"]
        #[serde(rename = "externalAgentConfig/import/progress")]
        ExternalAgentConfigImportProgress(ExternalAgentConfigImportProgressNotification),
        #[doc = "ExternalAgentConfig/import/completedNotification"]
        #[serde(rename = "externalAgentConfig/import/completed")]
        ExternalAgentConfigImportCompleted(ExternalAgentConfigImportCompletedNotification),
        #[doc = "Fs/changedNotification"]
        #[serde(rename = "fs/changed")]
        FsChanged(FsChangedNotification),
        #[doc = "Item/reasoning/summaryTextDeltaNotification"]
        #[serde(rename = "item/reasoning/summaryTextDelta")]
        ItemReasoningSummaryTextDelta(ReasoningSummaryTextDeltaNotification),
        #[doc = "Item/reasoning/summaryPartAddedNotification"]
        #[serde(rename = "item/reasoning/summaryPartAdded")]
        ItemReasoningSummaryPartAdded(ReasoningSummaryPartAddedNotification),
        #[doc = "Item/reasoning/textDeltaNotification"]
        #[serde(rename = "item/reasoning/textDelta")]
        ItemReasoningTextDelta(ReasoningTextDeltaNotification),
        #[doc = "Thread/compactedNotification\n\nDeprecated: Use `ContextCompaction` item type instead."]
        #[serde(rename = "thread/compacted")]
        ThreadCompacted(ContextCompactedNotification),
        #[doc = "Model/reroutedNotification"]
        #[serde(rename = "model/rerouted")]
        ModelRerouted(ModelReroutedNotification),
        #[doc = "Model/verificationNotification"]
        #[serde(rename = "model/verification")]
        ModelVerification(ModelVerificationNotification),
        #[doc = "Turn/moderationMetadataNotification"]
        #[serde(rename = "turn/moderationMetadata")]
        TurnModerationMetadata(TurnModerationMetadataNotification),
        #[doc = "Model/safetyBuffering/updatedNotification"]
        #[serde(rename = "model/safetyBuffering/updated")]
        ModelSafetyBufferingUpdated(ModelSafetyBufferingUpdatedNotification),
        #[doc = "WarningNotification"]
        #[serde(rename = "warning")]
        Warning(WarningNotification),
        #[doc = "GuardianWarningNotification"]
        #[serde(rename = "guardianWarning")]
        GuardianWarning(GuardianWarningNotification),
        #[doc = "DeprecationNoticeNotification"]
        #[serde(rename = "deprecationNotice")]
        DeprecationNotice(DeprecationNoticeNotification),
        #[doc = "ConfigWarningNotification"]
        #[serde(rename = "configWarning")]
        ConfigWarning(ConfigWarningNotification),
        #[doc = "FuzzyFileSearch/sessionUpdatedNotification"]
        #[serde(rename = "fuzzyFileSearch/sessionUpdated")]
        FuzzyFileSearchSessionUpdated(FuzzyFileSearchSessionUpdatedNotification),
        #[doc = "FuzzyFileSearch/sessionCompletedNotification"]
        #[serde(rename = "fuzzyFileSearch/sessionCompleted")]
        FuzzyFileSearchSessionCompleted(FuzzyFileSearchSessionCompletedNotification),
        #[doc = "Thread/realtime/startedNotification"]
        #[serde(rename = "thread/realtime/started")]
        ThreadRealtimeStarted(ThreadRealtimeStartedNotification),
        #[doc = "Thread/realtime/itemAddedNotification"]
        #[serde(rename = "thread/realtime/itemAdded")]
        ThreadRealtimeItemAdded(ThreadRealtimeItemAddedNotification),
        #[doc = "Thread/realtime/transcript/deltaNotification"]
        #[serde(rename = "thread/realtime/transcript/delta")]
        ThreadRealtimeTranscriptDelta(ThreadRealtimeTranscriptDeltaNotification),
        #[doc = "Thread/realtime/transcript/doneNotification"]
        #[serde(rename = "thread/realtime/transcript/done")]
        ThreadRealtimeTranscriptDone(ThreadRealtimeTranscriptDoneNotification),
        #[doc = "Thread/realtime/outputAudio/deltaNotification"]
        #[serde(rename = "thread/realtime/outputAudio/delta")]
        ThreadRealtimeOutputAudioDelta(ThreadRealtimeOutputAudioDeltaNotification),
        #[doc = "Thread/realtime/sdpNotification"]
        #[serde(rename = "thread/realtime/sdp")]
        ThreadRealtimeSdp(ThreadRealtimeSdpNotification),
        #[doc = "Thread/realtime/errorNotification"]
        #[serde(rename = "thread/realtime/error")]
        ThreadRealtimeError(ThreadRealtimeErrorNotification),
        #[doc = "Thread/realtime/closedNotification"]
        #[serde(rename = "thread/realtime/closed")]
        ThreadRealtimeClosed(ThreadRealtimeClosedNotification),
        #[doc = "Windows/worldWritableWarningNotification\n\nNotifies the user of world-writable directories on Windows, which cannot be protected by the sandbox."]
        #[serde(rename = "windows/worldWritableWarning")]
        WindowsWorldWritableWarning(WindowsWorldWritableWarningNotification),
        #[doc = "WindowsSandbox/setupCompletedNotification"]
        #[serde(rename = "windowsSandbox/setupCompleted")]
        WindowsSandboxSetupCompleted(WindowsSandboxSetupCompletedNotification),
        #[doc = "Account/login/completedNotification"]
        #[serde(rename = "account/login/completed")]
        AccountLoginCompleted(AccountLoginCompletedNotification),
    }
    impl ::std::convert::From<ErrorNotification> for ServerNotification {
        fn from(value: ErrorNotification) -> Self {
            Self::Error(value)
        }
    }
    impl ::std::convert::From<ThreadStartedNotification> for ServerNotification {
        fn from(value: ThreadStartedNotification) -> Self {
            Self::ThreadStarted(value)
        }
    }
    impl ::std::convert::From<ThreadStatusChangedNotification> for ServerNotification {
        fn from(value: ThreadStatusChangedNotification) -> Self {
            Self::ThreadStatusChanged(value)
        }
    }
    impl ::std::convert::From<ThreadArchivedNotification> for ServerNotification {
        fn from(value: ThreadArchivedNotification) -> Self {
            Self::ThreadArchived(value)
        }
    }
    impl ::std::convert::From<ThreadDeletedNotification> for ServerNotification {
        fn from(value: ThreadDeletedNotification) -> Self {
            Self::ThreadDeleted(value)
        }
    }
    impl ::std::convert::From<ThreadUnarchivedNotification> for ServerNotification {
        fn from(value: ThreadUnarchivedNotification) -> Self {
            Self::ThreadUnarchived(value)
        }
    }
    impl ::std::convert::From<ThreadClosedNotification> for ServerNotification {
        fn from(value: ThreadClosedNotification) -> Self {
            Self::ThreadClosed(value)
        }
    }
    impl ::std::convert::From<SkillsChangedNotification> for ServerNotification {
        fn from(value: SkillsChangedNotification) -> Self {
            Self::SkillsChanged(value)
        }
    }
    impl ::std::convert::From<ThreadNameUpdatedNotification> for ServerNotification {
        fn from(value: ThreadNameUpdatedNotification) -> Self {
            Self::ThreadNameUpdated(value)
        }
    }
    impl ::std::convert::From<ThreadGoalUpdatedNotification> for ServerNotification {
        fn from(value: ThreadGoalUpdatedNotification) -> Self {
            Self::ThreadGoalUpdated(value)
        }
    }
    impl ::std::convert::From<ThreadGoalClearedNotification> for ServerNotification {
        fn from(value: ThreadGoalClearedNotification) -> Self {
            Self::ThreadGoalCleared(value)
        }
    }
    impl ::std::convert::From<ThreadSettingsUpdatedNotification> for ServerNotification {
        fn from(value: ThreadSettingsUpdatedNotification) -> Self {
            Self::ThreadSettingsUpdated(value)
        }
    }
    impl ::std::convert::From<ThreadTokenUsageUpdatedNotification> for ServerNotification {
        fn from(value: ThreadTokenUsageUpdatedNotification) -> Self {
            Self::ThreadTokenUsageUpdated(value)
        }
    }
    impl ::std::convert::From<TurnStartedNotification> for ServerNotification {
        fn from(value: TurnStartedNotification) -> Self {
            Self::TurnStarted(value)
        }
    }
    impl ::std::convert::From<HookStartedNotification> for ServerNotification {
        fn from(value: HookStartedNotification) -> Self {
            Self::HookStarted(value)
        }
    }
    impl ::std::convert::From<TurnCompletedNotification> for ServerNotification {
        fn from(value: TurnCompletedNotification) -> Self {
            Self::TurnCompleted(value)
        }
    }
    impl ::std::convert::From<HookCompletedNotification> for ServerNotification {
        fn from(value: HookCompletedNotification) -> Self {
            Self::HookCompleted(value)
        }
    }
    impl ::std::convert::From<TurnDiffUpdatedNotification> for ServerNotification {
        fn from(value: TurnDiffUpdatedNotification) -> Self {
            Self::TurnDiffUpdated(value)
        }
    }
    impl ::std::convert::From<TurnPlanUpdatedNotification> for ServerNotification {
        fn from(value: TurnPlanUpdatedNotification) -> Self {
            Self::TurnPlanUpdated(value)
        }
    }
    impl ::std::convert::From<ItemStartedNotification> for ServerNotification {
        fn from(value: ItemStartedNotification) -> Self {
            Self::ItemStarted(value)
        }
    }
    impl ::std::convert::From<ItemGuardianApprovalReviewStartedNotification> for ServerNotification {
        fn from(value: ItemGuardianApprovalReviewStartedNotification) -> Self {
            Self::ItemAutoApprovalReviewStarted(value)
        }
    }
    impl ::std::convert::From<ItemGuardianApprovalReviewCompletedNotification> for ServerNotification {
        fn from(value: ItemGuardianApprovalReviewCompletedNotification) -> Self {
            Self::ItemAutoApprovalReviewCompleted(value)
        }
    }
    impl ::std::convert::From<ItemCompletedNotification> for ServerNotification {
        fn from(value: ItemCompletedNotification) -> Self {
            Self::ItemCompleted(value)
        }
    }
    impl ::std::convert::From<AgentMessageDeltaNotification> for ServerNotification {
        fn from(value: AgentMessageDeltaNotification) -> Self {
            Self::ItemAgentMessageDelta(value)
        }
    }
    impl ::std::convert::From<PlanDeltaNotification> for ServerNotification {
        fn from(value: PlanDeltaNotification) -> Self {
            Self::ItemPlanDelta(value)
        }
    }
    impl ::std::convert::From<CommandExecOutputDeltaNotification> for ServerNotification {
        fn from(value: CommandExecOutputDeltaNotification) -> Self {
            Self::CommandExecOutputDelta(value)
        }
    }
    impl ::std::convert::From<ProcessOutputDeltaNotification> for ServerNotification {
        fn from(value: ProcessOutputDeltaNotification) -> Self {
            Self::ProcessOutputDelta(value)
        }
    }
    impl ::std::convert::From<ProcessExitedNotification> for ServerNotification {
        fn from(value: ProcessExitedNotification) -> Self {
            Self::ProcessExited(value)
        }
    }
    impl ::std::convert::From<CommandExecutionOutputDeltaNotification> for ServerNotification {
        fn from(value: CommandExecutionOutputDeltaNotification) -> Self {
            Self::ItemCommandExecutionOutputDelta(value)
        }
    }
    impl ::std::convert::From<TerminalInteractionNotification> for ServerNotification {
        fn from(value: TerminalInteractionNotification) -> Self {
            Self::ItemCommandExecutionTerminalInteraction(value)
        }
    }
    impl ::std::convert::From<FileChangeOutputDeltaNotification> for ServerNotification {
        fn from(value: FileChangeOutputDeltaNotification) -> Self {
            Self::ItemFileChangeOutputDelta(value)
        }
    }
    impl ::std::convert::From<FileChangePatchUpdatedNotification> for ServerNotification {
        fn from(value: FileChangePatchUpdatedNotification) -> Self {
            Self::ItemFileChangePatchUpdated(value)
        }
    }
    impl ::std::convert::From<ServerRequestResolvedNotification> for ServerNotification {
        fn from(value: ServerRequestResolvedNotification) -> Self {
            Self::ServerRequestResolved(value)
        }
    }
    impl ::std::convert::From<McpToolCallProgressNotification> for ServerNotification {
        fn from(value: McpToolCallProgressNotification) -> Self {
            Self::ItemMcpToolCallProgress(value)
        }
    }
    impl ::std::convert::From<McpServerOauthLoginCompletedNotification> for ServerNotification {
        fn from(value: McpServerOauthLoginCompletedNotification) -> Self {
            Self::McpServerOauthLoginCompleted(value)
        }
    }
    impl ::std::convert::From<McpServerStatusUpdatedNotification> for ServerNotification {
        fn from(value: McpServerStatusUpdatedNotification) -> Self {
            Self::McpServerStartupStatusUpdated(value)
        }
    }
    impl ::std::convert::From<AccountUpdatedNotification> for ServerNotification {
        fn from(value: AccountUpdatedNotification) -> Self {
            Self::AccountUpdated(value)
        }
    }
    impl ::std::convert::From<AccountRateLimitsUpdatedNotification> for ServerNotification {
        fn from(value: AccountRateLimitsUpdatedNotification) -> Self {
            Self::AccountRateLimitsUpdated(value)
        }
    }
    impl ::std::convert::From<AppListUpdatedNotification> for ServerNotification {
        fn from(value: AppListUpdatedNotification) -> Self {
            Self::AppListUpdated(value)
        }
    }
    impl ::std::convert::From<RemoteControlStatusChangedNotification> for ServerNotification {
        fn from(value: RemoteControlStatusChangedNotification) -> Self {
            Self::RemoteControlStatusChanged(value)
        }
    }
    impl ::std::convert::From<ExternalAgentConfigImportProgressNotification> for ServerNotification {
        fn from(value: ExternalAgentConfigImportProgressNotification) -> Self {
            Self::ExternalAgentConfigImportProgress(value)
        }
    }
    impl ::std::convert::From<ExternalAgentConfigImportCompletedNotification> for ServerNotification {
        fn from(value: ExternalAgentConfigImportCompletedNotification) -> Self {
            Self::ExternalAgentConfigImportCompleted(value)
        }
    }
    impl ::std::convert::From<FsChangedNotification> for ServerNotification {
        fn from(value: FsChangedNotification) -> Self {
            Self::FsChanged(value)
        }
    }
    impl ::std::convert::From<ReasoningSummaryTextDeltaNotification> for ServerNotification {
        fn from(value: ReasoningSummaryTextDeltaNotification) -> Self {
            Self::ItemReasoningSummaryTextDelta(value)
        }
    }
    impl ::std::convert::From<ReasoningSummaryPartAddedNotification> for ServerNotification {
        fn from(value: ReasoningSummaryPartAddedNotification) -> Self {
            Self::ItemReasoningSummaryPartAdded(value)
        }
    }
    impl ::std::convert::From<ReasoningTextDeltaNotification> for ServerNotification {
        fn from(value: ReasoningTextDeltaNotification) -> Self {
            Self::ItemReasoningTextDelta(value)
        }
    }
    impl ::std::convert::From<ContextCompactedNotification> for ServerNotification {
        fn from(value: ContextCompactedNotification) -> Self {
            Self::ThreadCompacted(value)
        }
    }
    impl ::std::convert::From<ModelReroutedNotification> for ServerNotification {
        fn from(value: ModelReroutedNotification) -> Self {
            Self::ModelRerouted(value)
        }
    }
    impl ::std::convert::From<ModelVerificationNotification> for ServerNotification {
        fn from(value: ModelVerificationNotification) -> Self {
            Self::ModelVerification(value)
        }
    }
    impl ::std::convert::From<TurnModerationMetadataNotification> for ServerNotification {
        fn from(value: TurnModerationMetadataNotification) -> Self {
            Self::TurnModerationMetadata(value)
        }
    }
    impl ::std::convert::From<ModelSafetyBufferingUpdatedNotification> for ServerNotification {
        fn from(value: ModelSafetyBufferingUpdatedNotification) -> Self {
            Self::ModelSafetyBufferingUpdated(value)
        }
    }
    impl ::std::convert::From<WarningNotification> for ServerNotification {
        fn from(value: WarningNotification) -> Self {
            Self::Warning(value)
        }
    }
    impl ::std::convert::From<GuardianWarningNotification> for ServerNotification {
        fn from(value: GuardianWarningNotification) -> Self {
            Self::GuardianWarning(value)
        }
    }
    impl ::std::convert::From<DeprecationNoticeNotification> for ServerNotification {
        fn from(value: DeprecationNoticeNotification) -> Self {
            Self::DeprecationNotice(value)
        }
    }
    impl ::std::convert::From<ConfigWarningNotification> for ServerNotification {
        fn from(value: ConfigWarningNotification) -> Self {
            Self::ConfigWarning(value)
        }
    }
    impl ::std::convert::From<FuzzyFileSearchSessionUpdatedNotification> for ServerNotification {
        fn from(value: FuzzyFileSearchSessionUpdatedNotification) -> Self {
            Self::FuzzyFileSearchSessionUpdated(value)
        }
    }
    impl ::std::convert::From<FuzzyFileSearchSessionCompletedNotification> for ServerNotification {
        fn from(value: FuzzyFileSearchSessionCompletedNotification) -> Self {
            Self::FuzzyFileSearchSessionCompleted(value)
        }
    }
    impl ::std::convert::From<ThreadRealtimeStartedNotification> for ServerNotification {
        fn from(value: ThreadRealtimeStartedNotification) -> Self {
            Self::ThreadRealtimeStarted(value)
        }
    }
    impl ::std::convert::From<ThreadRealtimeItemAddedNotification> for ServerNotification {
        fn from(value: ThreadRealtimeItemAddedNotification) -> Self {
            Self::ThreadRealtimeItemAdded(value)
        }
    }
    impl ::std::convert::From<ThreadRealtimeTranscriptDeltaNotification> for ServerNotification {
        fn from(value: ThreadRealtimeTranscriptDeltaNotification) -> Self {
            Self::ThreadRealtimeTranscriptDelta(value)
        }
    }
    impl ::std::convert::From<ThreadRealtimeTranscriptDoneNotification> for ServerNotification {
        fn from(value: ThreadRealtimeTranscriptDoneNotification) -> Self {
            Self::ThreadRealtimeTranscriptDone(value)
        }
    }
    impl ::std::convert::From<ThreadRealtimeOutputAudioDeltaNotification> for ServerNotification {
        fn from(value: ThreadRealtimeOutputAudioDeltaNotification) -> Self {
            Self::ThreadRealtimeOutputAudioDelta(value)
        }
    }
    impl ::std::convert::From<ThreadRealtimeSdpNotification> for ServerNotification {
        fn from(value: ThreadRealtimeSdpNotification) -> Self {
            Self::ThreadRealtimeSdp(value)
        }
    }
    impl ::std::convert::From<ThreadRealtimeErrorNotification> for ServerNotification {
        fn from(value: ThreadRealtimeErrorNotification) -> Self {
            Self::ThreadRealtimeError(value)
        }
    }
    impl ::std::convert::From<ThreadRealtimeClosedNotification> for ServerNotification {
        fn from(value: ThreadRealtimeClosedNotification) -> Self {
            Self::ThreadRealtimeClosed(value)
        }
    }
    impl ::std::convert::From<WindowsWorldWritableWarningNotification> for ServerNotification {
        fn from(value: WindowsWorldWritableWarningNotification) -> Self {
            Self::WindowsWorldWritableWarning(value)
        }
    }
    impl ::std::convert::From<WindowsSandboxSetupCompletedNotification> for ServerNotification {
        fn from(value: WindowsSandboxSetupCompletedNotification) -> Self {
            Self::WindowsSandboxSetupCompleted(value)
        }
    }
    impl ::std::convert::From<AccountLoginCompletedNotification> for ServerNotification {
        fn from(value: AccountLoginCompletedNotification) -> Self {
            Self::AccountLoginCompleted(value)
        }
    }
    #[doc = "`ServerRequestResolvedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ServerRequestResolvedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"requestId\","]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"requestId\": {"]
    #[doc = "      \"$ref\": \"#/definitions/RequestId\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ServerRequestResolvedNotification {
        #[serde(rename = "requestId")]
        pub request_id: RequestId,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
    }
    #[doc = "`SessionSource`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"cli\","]
    #[doc = "        \"vscode\","]
    #[doc = "        \"exec\","]
    #[doc = "        \"appServer\","]
    #[doc = "        \"unknown\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"CustomSessionSource\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"custom\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"custom\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        }"]
    #[doc = "      },"]
    #[doc = "      \"additionalProperties\": false"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SubAgentSessionSource\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"subAgent\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"subAgent\": {"]
    #[doc = "          \"$ref\": \"#/definitions/SubAgentSource\""]
    #[doc = "        }"]
    #[doc = "      },"]
    #[doc = "      \"additionalProperties\": false"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub enum SessionSource {
        #[serde(rename = "cli")]
        Cli,
        #[serde(rename = "vscode")]
        Vscode,
        #[serde(rename = "exec")]
        Exec,
        #[serde(rename = "appServer")]
        AppServer,
        #[serde(rename = "unknown")]
        Unknown,
        #[serde(rename = "custom")]
        Custom(::std::string::String),
        #[serde(rename = "subAgent")]
        SubAgent(SubAgentSource),
    }
    impl ::std::convert::From<SubAgentSource> for SessionSource {
        fn from(value: SubAgentSource) -> Self {
            Self::SubAgent(value)
        }
    }
    #[doc = "Settings for a collaboration mode."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"Settings for a collaboration mode.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"model\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"developer_instructions\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"model\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"reasoning_effort\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/ReasoningEffort\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct Settings {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub developer_instructions: ::std::option::Option<::std::string::String>,
        pub model: ::std::string::String,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub reasoning_effort: ::std::option::Option<ReasoningEffort>,
    }
    #[doc = "Notification emitted when watched local skill files change.\n\nTreat this as an invalidation signal and re-run `skills/list` with the client's current parameters when refreshed skill metadata is needed."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"SkillsChangedNotification\","]
    #[doc = "  \"description\": \"Notification emitted when watched local skill files change.\\n\\nTreat this as an invalidation signal and re-run `skills/list` with the client's current parameters when refreshed skill metadata is needed.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(transparent)]
    pub struct SkillsChangedNotification(
        pub ::serde_json::Map<::std::string::String, ::serde_json::Value>,
    );
    impl ::std::ops::Deref for SkillsChangedNotification {
        type Target = ::serde_json::Map<::std::string::String, ::serde_json::Value>;
        fn deref(&self) -> &::serde_json::Map<::std::string::String, ::serde_json::Value> {
            &self.0
        }
    }
    impl ::std::convert::From<SkillsChangedNotification>
        for ::serde_json::Map<::std::string::String, ::serde_json::Value>
    {
        fn from(value: SkillsChangedNotification) -> Self {
            value.0
        }
    }
    impl ::std::convert::From<::serde_json::Map<::std::string::String, ::serde_json::Value>>
        for SkillsChangedNotification
    {
        fn from(value: ::serde_json::Map<::std::string::String, ::serde_json::Value>) -> Self {
            Self(value)
        }
    }
    #[doc = "`SpendControlLimitSnapshot`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"limit\","]
    #[doc = "    \"remainingPercent\","]
    #[doc = "    \"resetsAt\","]
    #[doc = "    \"used\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"limit\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"remainingPercent\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int32\""]
    #[doc = "    },"]
    #[doc = "    \"resetsAt\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"used\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct SpendControlLimitSnapshot {
        pub limit: ::std::string::String,
        #[serde(rename = "remainingPercent")]
        pub remaining_percent: i32,
        #[serde(rename = "resetsAt")]
        #[ts(type = "number")]
        pub resets_at: i64,
        pub used: ::std::string::String,
    }
    #[doc = "`SubAgentActivityKind`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"started\","]
    #[doc = "    \"interacted\","]
    #[doc = "    \"interrupted\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum SubAgentActivityKind {
        #[serde(rename = "started")]
        Started,
        #[serde(rename = "interacted")]
        Interacted,
        #[serde(rename = "interrupted")]
        Interrupted,
    }
    impl ::std::fmt::Display for SubAgentActivityKind {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Started => f.write_str("started"),
                Self::Interacted => f.write_str("interacted"),
                Self::Interrupted => f.write_str("interrupted"),
            }
        }
    }
    impl ::std::str::FromStr for SubAgentActivityKind {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "started" => Ok(Self::Started),
                "interacted" => Ok(Self::Interacted),
                "interrupted" => Ok(Self::Interrupted),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for SubAgentActivityKind {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for SubAgentActivityKind {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for SubAgentActivityKind {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`SubAgentSource`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"review\","]
    #[doc = "        \"compact\","]
    #[doc = "        \"memory_consolidation\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ThreadSpawnSubAgentSource\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"thread_spawn\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"thread_spawn\": {"]
    #[doc = "          \"type\": \"object\","]
    #[doc = "          \"required\": ["]
    #[doc = "            \"depth\","]
    #[doc = "            \"parent_thread_id\""]
    #[doc = "          ],"]
    #[doc = "          \"properties\": {"]
    #[doc = "            \"agent_nickname\": {"]
    #[doc = "              \"default\": null,"]
    #[doc = "              \"oneOf\": ["]
    #[doc = "                {"]
    #[doc = "                  \"default\": null,"]
    #[doc = "                  \"type\": \"string\""]
    #[doc = "                },"]
    #[doc = "                {"]
    #[doc = "                  \"type\": \"null\""]
    #[doc = "                }"]
    #[doc = "              ]"]
    #[doc = "            },"]
    #[doc = "            \"agent_path\": {"]
    #[doc = "              \"default\": null,"]
    #[doc = "              \"anyOf\": ["]
    #[doc = "                {"]
    #[doc = "                  \"$ref\": \"#/definitions/AgentPath\""]
    #[doc = "                },"]
    #[doc = "                {"]
    #[doc = "                  \"type\": \"null\""]
    #[doc = "                }"]
    #[doc = "              ]"]
    #[doc = "            },"]
    #[doc = "            \"agent_role\": {"]
    #[doc = "              \"default\": null,"]
    #[doc = "              \"oneOf\": ["]
    #[doc = "                {"]
    #[doc = "                  \"default\": null,"]
    #[doc = "                  \"type\": \"string\""]
    #[doc = "                },"]
    #[doc = "                {"]
    #[doc = "                  \"type\": \"null\""]
    #[doc = "                }"]
    #[doc = "              ]"]
    #[doc = "            },"]
    #[doc = "            \"depth\": {"]
    #[doc = "              \"type\": \"integer\","]
    #[doc = "              \"format\": \"int32\""]
    #[doc = "            },"]
    #[doc = "            \"parent_thread_id\": {"]
    #[doc = "              \"$ref\": \"#/definitions/ThreadId\""]
    #[doc = "            }"]
    #[doc = "          }"]
    #[doc = "        }"]
    #[doc = "      },"]
    #[doc = "      \"additionalProperties\": false"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"OtherSubAgentSource\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"other\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"other\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        }"]
    #[doc = "      },"]
    #[doc = "      \"additionalProperties\": false"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub enum SubAgentSource {
        #[serde(rename = "review")]
        Review,
        #[serde(rename = "compact")]
        Compact,
        #[serde(rename = "memory_consolidation")]
        MemoryConsolidation,
        #[serde(rename = "thread_spawn")]
        ThreadSpawn {
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            agent_nickname: ::std::option::Option<::std::string::String>,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            agent_path: ::std::option::Option<AgentPath>,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            agent_role: ::std::option::Option<::std::string::String>,
            depth: i32,
            parent_thread_id: ThreadId,
        },
        #[serde(rename = "other")]
        Other(::std::string::String),
    }
    #[doc = "`TerminalInteractionNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"TerminalInteractionNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"itemId\","]
    #[doc = "    \"processId\","]
    #[doc = "    \"stdin\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"itemId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"processId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"stdin\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct TerminalInteractionNotification {
        #[serde(rename = "itemId")]
        pub item_id: ::std::string::String,
        #[serde(rename = "processId")]
        pub process_id: ::std::string::String,
        pub stdin: ::std::string::String,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`TextElement`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"byteRange\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"byteRange\": {"]
    #[doc = "      \"description\": \"Byte range in the parent `text` buffer that this element occupies.\","]
    #[doc = "      \"allOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/ByteRange\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"placeholder\": {"]
    #[doc = "      \"description\": \"Optional human-readable placeholder for the element, displayed in the UI.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Optional human-readable placeholder for the element, displayed in the UI.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct TextElement {
        #[doc = "Byte range in the parent `text` buffer that this element occupies."]
        #[serde(rename = "byteRange")]
        pub byte_range: ByteRange,
        #[doc = "Optional human-readable placeholder for the element, displayed in the UI."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub placeholder: ::std::option::Option<::std::option::Option<::std::string::String>>,
    }
    #[doc = "`TextPosition`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"column\","]
    #[doc = "    \"line\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"column\": {"]
    #[doc = "      \"description\": \"1-based column number (in Unicode scalar values).\","]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"uint\","]
    #[doc = "      \"minimum\": 0.0"]
    #[doc = "    },"]
    #[doc = "    \"line\": {"]
    #[doc = "      \"description\": \"1-based line number.\","]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"uint\","]
    #[doc = "      \"minimum\": 0.0"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct TextPosition {
        #[doc = "1-based column number (in Unicode scalar values)."]
        pub column: u32,
        #[doc = "1-based line number."]
        pub line: u32,
    }
    #[doc = "`TextRange`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"end\","]
    #[doc = "    \"start\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"end\": {"]
    #[doc = "      \"$ref\": \"#/definitions/TextPosition\""]
    #[doc = "    },"]
    #[doc = "    \"start\": {"]
    #[doc = "      \"$ref\": \"#/definitions/TextPosition\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct TextRange {
        pub end: TextPosition,
        pub start: TextPosition,
    }
    #[doc = "`Thread`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"cliVersion\","]
    #[doc = "    \"createdAt\","]
    #[doc = "    \"cwd\","]
    #[doc = "    \"ephemeral\","]
    #[doc = "    \"id\","]
    #[doc = "    \"modelProvider\","]
    #[doc = "    \"preview\","]
    #[doc = "    \"sessionId\","]
    #[doc = "    \"source\","]
    #[doc = "    \"status\","]
    #[doc = "    \"turns\","]
    #[doc = "    \"updatedAt\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"agentNickname\": {"]
    #[doc = "      \"description\": \"Optional random unique nickname assigned to an AgentControl-spawned sub-agent.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Optional random unique nickname assigned to an AgentControl-spawned sub-agent.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"agentRole\": {"]
    #[doc = "      \"description\": \"Optional role (agent_role) assigned to an AgentControl-spawned sub-agent.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Optional role (agent_role) assigned to an AgentControl-spawned sub-agent.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"cliVersion\": {"]
    #[doc = "      \"description\": \"Version of the CLI that created the thread.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"createdAt\": {"]
    #[doc = "      \"description\": \"Unix timestamp (in seconds) when the thread was created.\","]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"cwd\": {"]
    #[doc = "      \"description\": \"Working directory captured for the thread.\","]
    #[doc = "      \"allOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/AbsolutePathBuf\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"ephemeral\": {"]
    #[doc = "      \"description\": \"Whether the thread is ephemeral and should not be materialized on disk.\","]
    #[doc = "      \"type\": \"boolean\""]
    #[doc = "    },"]
    #[doc = "    \"forkedFromId\": {"]
    #[doc = "      \"description\": \"Source thread id when this thread was created by forking another thread.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Source thread id when this thread was created by forking another thread.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"gitInfo\": {"]
    #[doc = "      \"description\": \"Optional Git metadata captured when the thread was created.\","]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/GitInfo\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"id\": {"]
    #[doc = "      \"description\": \"Identifier for this thread. Codex-generated thread IDs are UUIDv7.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"modelProvider\": {"]
    #[doc = "      \"description\": \"Model provider used for this thread (for example, 'openai').\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"name\": {"]
    #[doc = "      \"description\": \"Optional user-facing thread title.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Optional user-facing thread title.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"parentThreadId\": {"]
    #[doc = "      \"description\": \"The ID of the parent thread. This will only be set if this thread is a subagent.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"The ID of the parent thread. This will only be set if this thread is a subagent.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"path\": {"]
    #[doc = "      \"description\": \"[UNSTABLE] Path to the thread on disk.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"[UNSTABLE] Path to the thread on disk.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"preview\": {"]
    #[doc = "      \"description\": \"Usually the first user message in the thread, if available.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"recencyAt\": {"]
    #[doc = "      \"description\": \"Unix timestamp (in seconds) used for thread recency ordering.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Unix timestamp (in seconds) used for thread recency ordering.\","]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"int64\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"sessionId\": {"]
    #[doc = "      \"description\": \"Session id shared by threads that belong to the same session tree.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"source\": {"]
    #[doc = "      \"description\": \"Origin of the thread (CLI, VSCode, codex exec, codex app-server, etc.).\","]
    #[doc = "      \"allOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/SessionSource\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"status\": {"]
    #[doc = "      \"description\": \"Current runtime status for the thread.\","]
    #[doc = "      \"allOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadStatus\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"threadSource\": {"]
    #[doc = "      \"description\": \"Optional analytics source classification for this thread.\","]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/ThreadSource\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"turns\": {"]
    #[doc = "      \"description\": \"Only populated on `thread/resume`, `thread/rollback`, `thread/fork`, and `thread/read` (when `includeTurns` is true) responses. For all other responses and notifications returning a Thread, the turns field will be an empty list.\","]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"$ref\": \"#/definitions/Turn\""]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    \"updatedAt\": {"]
    #[doc = "      \"description\": \"Unix timestamp (in seconds) when the thread was last updated.\","]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct Thread {
        #[doc = "Optional random unique nickname assigned to an AgentControl-spawned sub-agent."]
        #[serde(
            rename = "agentNickname",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub agent_nickname: ::std::option::Option<::std::string::String>,
        #[doc = "Optional role (agent_role) assigned to an AgentControl-spawned sub-agent."]
        #[serde(
            rename = "agentRole",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub agent_role: ::std::option::Option<::std::string::String>,
        #[doc = "Version of the CLI that created the thread."]
        #[serde(rename = "cliVersion")]
        pub cli_version: ::std::string::String,
        #[doc = "Unix timestamp (in seconds) when the thread was created."]
        #[serde(rename = "createdAt")]
        #[ts(type = "number")]
        pub created_at: i64,
        #[doc = "Working directory captured for the thread."]
        pub cwd: AbsolutePathBuf,
        #[doc = "Whether the thread is ephemeral and should not be materialized on disk."]
        pub ephemeral: bool,
        #[doc = "Source thread id when this thread was created by forking another thread."]
        #[serde(
            rename = "forkedFromId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub forked_from_id: ::std::option::Option<::std::string::String>,
        #[doc = "Optional Git metadata captured when the thread was created."]
        #[serde(
            rename = "gitInfo",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub git_info: ::std::option::Option<GitInfo>,
        #[doc = "Identifier for this thread. Codex-generated thread IDs are UUIDv7."]
        pub id: ::std::string::String,
        #[doc = "Model provider used for this thread (for example, 'openai')."]
        #[serde(rename = "modelProvider")]
        pub model_provider: ::std::string::String,
        #[doc = "Optional user-facing thread title."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub name: ::std::option::Option<::std::string::String>,
        #[doc = "The ID of the parent thread. This will only be set if this thread is a subagent."]
        #[serde(
            rename = "parentThreadId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub parent_thread_id: ::std::option::Option<::std::string::String>,
        #[doc = "[UNSTABLE] Path to the thread on disk."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub path: ::std::option::Option<::std::string::String>,
        #[doc = "Usually the first user message in the thread, if available."]
        pub preview: ::std::string::String,
        #[doc = "Unix timestamp (in seconds) used for thread recency ordering."]
        #[serde(
            rename = "recencyAt",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[ts(type = "number | null")]
        pub recency_at: ::std::option::Option<i64>,
        #[doc = "Session id shared by threads that belong to the same session tree."]
        #[serde(rename = "sessionId")]
        pub session_id: ::std::string::String,
        #[doc = "Origin of the thread (CLI, VSCode, codex exec, codex app-server, etc.)."]
        pub source: SessionSource,
        #[doc = "Current runtime status for the thread."]
        pub status: ThreadStatus,
        #[doc = "Optional analytics source classification for this thread."]
        #[serde(
            rename = "threadSource",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub thread_source: ::std::option::Option<ThreadSource>,
        #[doc = "Only populated on `thread/resume`, `thread/rollback`, `thread/fork`, and `thread/read` (when `includeTurns` is true) responses. For all other responses and notifications returning a Thread, the turns field will be an empty list."]
        pub turns: ::std::vec::Vec<Turn>,
        #[doc = "Unix timestamp (in seconds) when the thread was last updated."]
        #[serde(rename = "updatedAt")]
        #[ts(type = "number")]
        pub updated_at: i64,
    }
    #[doc = "`ThreadActiveFlag`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"waitingOnApproval\","]
    #[doc = "    \"waitingOnUserInput\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum ThreadActiveFlag {
        #[serde(rename = "waitingOnApproval")]
        WaitingOnApproval,
        #[serde(rename = "waitingOnUserInput")]
        WaitingOnUserInput,
    }
    impl ::std::fmt::Display for ThreadActiveFlag {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::WaitingOnApproval => f.write_str("waitingOnApproval"),
                Self::WaitingOnUserInput => f.write_str("waitingOnUserInput"),
            }
        }
    }
    impl ::std::str::FromStr for ThreadActiveFlag {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "waitingOnApproval" => Ok(Self::WaitingOnApproval),
                "waitingOnUserInput" => Ok(Self::WaitingOnUserInput),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for ThreadActiveFlag {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for ThreadActiveFlag {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for ThreadActiveFlag {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`ThreadArchivedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadArchivedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadArchivedNotification {
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
    }
    #[doc = "`ThreadClosedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadClosedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadClosedNotification {
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
    }
    #[doc = "`ThreadDeletedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadDeletedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadDeletedNotification {
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
    }
    #[doc = "`ThreadGoal`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"createdAt\","]
    #[doc = "    \"objective\","]
    #[doc = "    \"status\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"timeUsedSeconds\","]
    #[doc = "    \"tokensUsed\","]
    #[doc = "    \"updatedAt\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"createdAt\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"objective\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"status\": {"]
    #[doc = "      \"$ref\": \"#/definitions/ThreadGoalStatus\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"timeUsedSeconds\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"tokenBudget\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"int64\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"tokensUsed\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"updatedAt\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadGoal {
        #[serde(rename = "createdAt")]
        #[ts(type = "number")]
        pub created_at: i64,
        pub objective: ::std::string::String,
        pub status: ThreadGoalStatus,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "timeUsedSeconds")]
        #[ts(type = "number")]
        pub time_used_seconds: i64,
        #[serde(
            rename = "tokenBudget",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[ts(type = "number | null")]
        pub token_budget: ::std::option::Option<i64>,
        #[serde(rename = "tokensUsed")]
        #[ts(type = "number")]
        pub tokens_used: i64,
        #[serde(rename = "updatedAt")]
        #[ts(type = "number")]
        pub updated_at: i64,
    }
    #[doc = "`ThreadGoalClearedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadGoalClearedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadGoalClearedNotification {
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
    }
    #[doc = "`ThreadGoalStatus`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"active\","]
    #[doc = "    \"paused\","]
    #[doc = "    \"blocked\","]
    #[doc = "    \"usageLimited\","]
    #[doc = "    \"budgetLimited\","]
    #[doc = "    \"complete\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum ThreadGoalStatus {
        #[serde(rename = "active")]
        Active,
        #[serde(rename = "paused")]
        Paused,
        #[serde(rename = "blocked")]
        Blocked,
        #[serde(rename = "usageLimited")]
        UsageLimited,
        #[serde(rename = "budgetLimited")]
        BudgetLimited,
        #[serde(rename = "complete")]
        Complete,
    }
    impl ::std::fmt::Display for ThreadGoalStatus {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Active => f.write_str("active"),
                Self::Paused => f.write_str("paused"),
                Self::Blocked => f.write_str("blocked"),
                Self::UsageLimited => f.write_str("usageLimited"),
                Self::BudgetLimited => f.write_str("budgetLimited"),
                Self::Complete => f.write_str("complete"),
            }
        }
    }
    impl ::std::str::FromStr for ThreadGoalStatus {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "active" => Ok(Self::Active),
                "paused" => Ok(Self::Paused),
                "blocked" => Ok(Self::Blocked),
                "usageLimited" => Ok(Self::UsageLimited),
                "budgetLimited" => Ok(Self::BudgetLimited),
                "complete" => Ok(Self::Complete),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for ThreadGoalStatus {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for ThreadGoalStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for ThreadGoalStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`ThreadGoalUpdatedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadGoalUpdatedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"goal\","]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"goal\": {"]
    #[doc = "      \"$ref\": \"#/definitions/ThreadGoal\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadGoalUpdatedNotification {
        pub goal: ThreadGoal,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(
            rename = "turnId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub turn_id: ::std::option::Option<::std::string::String>,
    }
    #[doc = "`ThreadId`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    #[serde(transparent)]
    pub struct ThreadId(pub ::std::string::String);
    impl ::std::ops::Deref for ThreadId {
        type Target = ::std::string::String;
        fn deref(&self) -> &::std::string::String {
            &self.0
        }
    }
    impl ::std::convert::From<ThreadId> for ::std::string::String {
        fn from(value: ThreadId) -> Self {
            value.0
        }
    }
    impl ::std::convert::From<::std::string::String> for ThreadId {
        fn from(value: ::std::string::String) -> Self {
            Self(value)
        }
    }
    impl ::std::str::FromStr for ThreadId {
        type Err = ::std::convert::Infallible;
        fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
            Ok(Self(value.to_string()))
        }
    }
    impl ::std::fmt::Display for ThreadId {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            self.0.fmt(f)
        }
    }
    #[doc = "`ThreadItem`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"UserMessageThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"content\","]
    #[doc = "        \"id\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"clientId\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"content\": {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/UserInput\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"UserMessageThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"userMessage\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"HookPromptThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"fragments\","]
    #[doc = "        \"id\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"fragments\": {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/HookPromptFragment\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"HookPromptThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"hookPrompt\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"AgentMessageThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"id\","]
    #[doc = "        \"text\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"memoryCitation\": {"]
    #[doc = "          \"default\": null,"]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/MemoryCitation\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"phase\": {"]
    #[doc = "          \"default\": null,"]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/MessagePhase\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"text\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"AgentMessageThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"agentMessage\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"PlanThreadItem\","]
    #[doc = "      \"description\": \"EXPERIMENTAL - proposed plan item content. The completed plan item is authoritative and may not match the concatenation of `PlanDelta` text.\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"id\","]
    #[doc = "        \"text\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"text\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"PlanThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"plan\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ReasoningThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"id\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"content\": {"]
    #[doc = "          \"default\": [],"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"type\": \"string\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"summary\": {"]
    #[doc = "          \"default\": [],"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"type\": \"string\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ReasoningThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"reasoning\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"CommandExecutionThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"command\","]
    #[doc = "        \"commandActions\","]
    #[doc = "        \"cwd\","]
    #[doc = "        \"id\","]
    #[doc = "        \"status\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"aggregatedOutput\": {"]
    #[doc = "          \"description\": \"The command's output, aggregated from stdout and stderr.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"The command's output, aggregated from stdout and stderr.\","]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"command\": {"]
    #[doc = "          \"description\": \"The command to be executed.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"commandActions\": {"]
    #[doc = "          \"description\": \"A best-effort parsing of the command to understand the action(s) it will perform. This returns a list of CommandAction objects because a single shell command may be composed of many commands piped together.\","]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/CommandAction\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"cwd\": {"]
    #[doc = "          \"description\": \"The command's working directory.\","]
    #[doc = "          \"allOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/LegacyAppPathString\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"durationMs\": {"]
    #[doc = "          \"description\": \"The duration of the command execution in milliseconds.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"The duration of the command execution in milliseconds.\","]
    #[doc = "              \"type\": \"integer\","]
    #[doc = "              \"format\": \"int64\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"exitCode\": {"]
    #[doc = "          \"description\": \"The command's exit code.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"The command's exit code.\","]
    #[doc = "              \"type\": \"integer\","]
    #[doc = "              \"format\": \"int32\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"processId\": {"]
    #[doc = "          \"description\": \"Identifier for the underlying PTY process (when available).\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"Identifier for the underlying PTY process (when available).\","]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"source\": {"]
    #[doc = "          \"default\": \"agent\","]
    #[doc = "          \"allOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/CommandExecutionSource\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"status\": {"]
    #[doc = "          \"$ref\": \"#/definitions/CommandExecutionStatus\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"CommandExecutionThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"commandExecution\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"FileChangeThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"changes\","]
    #[doc = "        \"id\","]
    #[doc = "        \"status\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"changes\": {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/FileUpdateChange\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"status\": {"]
    #[doc = "          \"$ref\": \"#/definitions/PatchApplyStatus\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"FileChangeThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"fileChange\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"McpToolCallThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"arguments\","]
    #[doc = "        \"id\","]
    #[doc = "        \"server\","]
    #[doc = "        \"status\","]
    #[doc = "        \"tool\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"appContext\": {"]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/McpToolCallAppContext\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"arguments\": true,"]
    #[doc = "        \"durationMs\": {"]
    #[doc = "          \"description\": \"The duration of the MCP tool call in milliseconds.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"The duration of the MCP tool call in milliseconds.\","]
    #[doc = "              \"type\": \"integer\","]
    #[doc = "              \"format\": \"int64\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"error\": {"]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/McpToolCallError\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"mcpAppResourceUri\": {"]
    #[doc = "          \"description\": \"Deprecated: use `appContext.resourceUri` instead.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"Deprecated: use `appContext.resourceUri` instead.\","]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"pluginId\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"result\": {"]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/McpToolCallResult\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"server\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"status\": {"]
    #[doc = "          \"$ref\": \"#/definitions/McpToolCallStatus\""]
    #[doc = "        },"]
    #[doc = "        \"tool\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"McpToolCallThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"mcpToolCall\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"DynamicToolCallThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"arguments\","]
    #[doc = "        \"id\","]
    #[doc = "        \"status\","]
    #[doc = "        \"tool\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"arguments\": true,"]
    #[doc = "        \"contentItems\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"array\","]
    #[doc = "              \"items\": {"]
    #[doc = "                \"$ref\": \"#/definitions/DynamicToolCallOutputContentItem\""]
    #[doc = "              }"]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"durationMs\": {"]
    #[doc = "          \"description\": \"The duration of the dynamic tool call in milliseconds.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"The duration of the dynamic tool call in milliseconds.\","]
    #[doc = "              \"type\": \"integer\","]
    #[doc = "              \"format\": \"int64\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"namespace\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"status\": {"]
    #[doc = "          \"$ref\": \"#/definitions/DynamicToolCallStatus\""]
    #[doc = "        },"]
    #[doc = "        \"success\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"boolean\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"tool\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"DynamicToolCallThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"dynamicToolCall\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"CollabAgentToolCallThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"agentsStates\","]
    #[doc = "        \"id\","]
    #[doc = "        \"receiverThreadIds\","]
    #[doc = "        \"senderThreadId\","]
    #[doc = "        \"status\","]
    #[doc = "        \"tool\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"agentsStates\": {"]
    #[doc = "          \"description\": \"Last known status of the target agents, when available.\","]
    #[doc = "          \"type\": \"object\","]
    #[doc = "          \"additionalProperties\": {"]
    #[doc = "            \"$ref\": \"#/definitions/CollabAgentState\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"description\": \"Unique identifier for this collab tool call.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"model\": {"]
    #[doc = "          \"description\": \"Model requested for the spawned agent, when applicable.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"Model requested for the spawned agent, when applicable.\","]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"prompt\": {"]
    #[doc = "          \"description\": \"Prompt text sent as part of the collab tool call, when available.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"Prompt text sent as part of the collab tool call, when available.\","]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"reasoningEffort\": {"]
    #[doc = "          \"description\": \"Reasoning effort requested for the spawned agent, when applicable.\","]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/ReasoningEffort\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"receiverThreadIds\": {"]
    #[doc = "          \"description\": \"Thread ID of the receiving agent, when applicable. In case of spawn operation, this corresponds to the newly spawned agent.\","]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"type\": \"string\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"senderThreadId\": {"]
    #[doc = "          \"description\": \"Thread ID of the agent issuing the collab request.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"status\": {"]
    #[doc = "          \"description\": \"Current status of the collab tool call.\","]
    #[doc = "          \"allOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/CollabAgentToolCallStatus\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"tool\": {"]
    #[doc = "          \"description\": \"Name of the collab tool that was invoked.\","]
    #[doc = "          \"allOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/CollabAgentTool\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"CollabAgentToolCallThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"collabAgentToolCall\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SubAgentActivityThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"agentPath\","]
    #[doc = "        \"agentThreadId\","]
    #[doc = "        \"id\","]
    #[doc = "        \"kind\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"agentPath\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"agentThreadId\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"$ref\": \"#/definitions/SubAgentActivityKind\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"SubAgentActivityThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"subAgentActivity\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"WebSearchThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"id\","]
    #[doc = "        \"query\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"action\": {"]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/WebSearchAction\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"query\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"WebSearchThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"webSearch\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ImageViewThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"id\","]
    #[doc = "        \"path\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"$ref\": \"#/definitions/LegacyAppPathString\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ImageViewThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"imageView\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SleepThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"durationMs\","]
    #[doc = "        \"id\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"durationMs\": {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"uint64\","]
    #[doc = "          \"minimum\": 0.0"]
    #[doc = "        },"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"SleepThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"sleep\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ImageGenerationThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"id\","]
    #[doc = "        \"result\","]
    #[doc = "        \"status\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"result\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"revisedPrompt\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"savedPath\": {"]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/AbsolutePathBuf\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"status\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ImageGenerationThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"imageGeneration\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"EnteredReviewModeThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"id\","]
    #[doc = "        \"review\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"review\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"EnteredReviewModeThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"enteredReviewMode\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ExitedReviewModeThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"id\","]
    #[doc = "        \"review\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"review\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ExitedReviewModeThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"exitedReviewMode\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ContextCompactionThreadItem\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"id\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"id\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ContextCompactionThreadItemType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"contextCompaction\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type")]
    pub enum ThreadItem {
        #[doc = "UserMessageThreadItem"]
        #[serde(rename = "userMessage")]
        UserMessage {
            #[serde(
                rename = "clientId",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            client_id: ::std::option::Option<::std::option::Option<::std::string::String>>,
            content: ::std::vec::Vec<UserInput>,
            id: ::std::string::String,
        },
        #[doc = "HookPromptThreadItem"]
        #[serde(rename = "hookPrompt")]
        HookPrompt {
            fragments: ::std::vec::Vec<HookPromptFragment>,
            id: ::std::string::String,
        },
        #[doc = "AgentMessageThreadItem"]
        #[serde(rename = "agentMessage")]
        AgentMessage {
            id: ::std::string::String,
            #[serde(
                rename = "memoryCitation",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            memory_citation: ::std::option::Option<::std::option::Option<MemoryCitation>>,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            phase: ::std::option::Option<::std::option::Option<MessagePhase>>,
            text: ::std::string::String,
        },
        #[doc = "PlanThreadItem\n\nEXPERIMENTAL - proposed plan item content. The completed plan item is authoritative and may not match the concatenation of `PlanDelta` text."]
        #[serde(rename = "plan")]
        Plan {
            id: ::std::string::String,
            text: ::std::string::String,
        },
        #[doc = "ReasoningThreadItem"]
        #[serde(rename = "reasoning")]
        Reasoning {
            #[serde(default)]
            content: ::std::vec::Vec<::std::string::String>,
            id: ::std::string::String,
            #[serde(default)]
            summary: ::std::vec::Vec<::std::string::String>,
        },
        #[doc = "CommandExecutionThreadItem"]
        #[serde(rename = "commandExecution")]
        CommandExecution {
            #[doc = "The command's output, aggregated from stdout and stderr."]
            #[serde(
                rename = "aggregatedOutput",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            aggregated_output: ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[doc = "The command to be executed."]
            command: ::std::string::String,
            #[doc = "A best-effort parsing of the command to understand the action(s) it will perform. This returns a list of CommandAction objects because a single shell command may be composed of many commands piped together."]
            #[serde(rename = "commandActions")]
            command_actions: ::std::vec::Vec<CommandAction>,
            #[doc = "The command's working directory."]
            cwd: LegacyAppPathString,
            #[doc = "The duration of the command execution in milliseconds."]
            #[serde(
                rename = "durationMs",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            #[ts(type = "number | null")]
            duration_ms: ::std::option::Option<::std::option::Option<i64>>,
            #[doc = "The command's exit code."]
            #[serde(
                rename = "exitCode",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            exit_code: ::std::option::Option<::std::option::Option<i32>>,
            id: ::std::string::String,
            #[doc = "Identifier for the underlying PTY process (when available)."]
            #[serde(
                rename = "processId",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            process_id: ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[serde(default = "defaults::thread_item_command_execution_source")]
            source: CommandExecutionSource,
            status: CommandExecutionStatus,
        },
        #[doc = "FileChangeThreadItem"]
        #[serde(rename = "fileChange")]
        FileChange {
            changes: ::std::vec::Vec<FileUpdateChange>,
            id: ::std::string::String,
            status: PatchApplyStatus,
        },
        #[doc = "McpToolCallThreadItem"]
        #[serde(rename = "mcpToolCall")]
        McpToolCall {
            #[serde(
                rename = "appContext",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            app_context: ::std::option::Option<::std::option::Option<McpToolCallAppContext>>,
            arguments: ::serde_json::Value,
            #[doc = "The duration of the MCP tool call in milliseconds."]
            #[serde(
                rename = "durationMs",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            #[ts(type = "number | null")]
            duration_ms: ::std::option::Option<::std::option::Option<i64>>,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            error: ::std::option::Option<::std::option::Option<McpToolCallError>>,
            id: ::std::string::String,
            #[doc = "Deprecated: use `appContext.resourceUri` instead."]
            #[serde(
                rename = "mcpAppResourceUri",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            mcp_app_resource_uri:
                ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[serde(
                rename = "pluginId",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            plugin_id: ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            result: ::std::option::Option<::std::option::Option<McpToolCallResult>>,
            server: ::std::string::String,
            status: McpToolCallStatus,
            tool: ::std::string::String,
        },
        #[doc = "DynamicToolCallThreadItem"]
        #[serde(rename = "dynamicToolCall")]
        DynamicToolCall {
            arguments: ::serde_json::Value,
            #[serde(
                rename = "contentItems",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            content_items: ::std::option::Option<
                ::std::option::Option<::std::vec::Vec<DynamicToolCallOutputContentItem>>,
            >,
            #[doc = "The duration of the dynamic tool call in milliseconds."]
            #[serde(
                rename = "durationMs",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            #[ts(type = "number | null")]
            duration_ms: ::std::option::Option<::std::option::Option<i64>>,
            id: ::std::string::String,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            namespace: ::std::option::Option<::std::option::Option<::std::string::String>>,
            status: DynamicToolCallStatus,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            success: ::std::option::Option<::std::option::Option<bool>>,
            tool: ::std::string::String,
        },
        #[doc = "CollabAgentToolCallThreadItem"]
        #[serde(rename = "collabAgentToolCall")]
        CollabAgentToolCall {
            #[doc = "Last known status of the target agents, when available."]
            #[serde(rename = "agentsStates")]
            agents_states: ::std::collections::HashMap<::std::string::String, CollabAgentState>,
            #[doc = "Unique identifier for this collab tool call."]
            id: ::std::string::String,
            #[doc = "Model requested for the spawned agent, when applicable."]
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            model: ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[doc = "Prompt text sent as part of the collab tool call, when available."]
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            prompt: ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[doc = "Reasoning effort requested for the spawned agent, when applicable."]
            #[serde(
                rename = "reasoningEffort",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            reasoning_effort: ::std::option::Option<::std::option::Option<ReasoningEffort>>,
            #[doc = "Thread ID of the receiving agent, when applicable. In case of spawn operation, this corresponds to the newly spawned agent."]
            #[serde(rename = "receiverThreadIds")]
            receiver_thread_ids: ::std::vec::Vec<::std::string::String>,
            #[doc = "Thread ID of the agent issuing the collab request."]
            #[serde(rename = "senderThreadId")]
            sender_thread_id: ::std::string::String,
            #[doc = "Current status of the collab tool call."]
            status: CollabAgentToolCallStatus,
            #[doc = "Name of the collab tool that was invoked."]
            tool: CollabAgentTool,
        },
        #[doc = "SubAgentActivityThreadItem"]
        #[serde(rename = "subAgentActivity")]
        SubAgentActivity {
            #[serde(rename = "agentPath")]
            agent_path: ::std::string::String,
            #[serde(rename = "agentThreadId")]
            agent_thread_id: ::std::string::String,
            id: ::std::string::String,
            kind: SubAgentActivityKind,
        },
        #[doc = "WebSearchThreadItem"]
        #[serde(rename = "webSearch")]
        WebSearch {
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            action: ::std::option::Option<::std::option::Option<WebSearchAction>>,
            id: ::std::string::String,
            query: ::std::string::String,
        },
        #[doc = "ImageViewThreadItem"]
        #[serde(rename = "imageView")]
        ImageView {
            id: ::std::string::String,
            path: LegacyAppPathString,
        },
        #[doc = "SleepThreadItem"]
        #[serde(rename = "sleep")]
        Sleep {
            #[serde(rename = "durationMs")]
            #[ts(type = "number")]
            duration_ms: u64,
            id: ::std::string::String,
        },
        #[doc = "ImageGenerationThreadItem"]
        #[serde(rename = "imageGeneration")]
        ImageGeneration {
            id: ::std::string::String,
            result: ::std::string::String,
            #[serde(
                rename = "revisedPrompt",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            revised_prompt: ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[serde(
                rename = "savedPath",
                default,
                skip_serializing_if = "::std::option::Option::is_none"
            )]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            saved_path: ::std::option::Option<::std::option::Option<AbsolutePathBuf>>,
            status: ::std::string::String,
        },
        #[doc = "EnteredReviewModeThreadItem"]
        #[serde(rename = "enteredReviewMode")]
        EnteredReviewMode {
            id: ::std::string::String,
            review: ::std::string::String,
        },
        #[doc = "ExitedReviewModeThreadItem"]
        #[serde(rename = "exitedReviewMode")]
        ExitedReviewMode {
            id: ::std::string::String,
            review: ::std::string::String,
        },
        #[doc = "ContextCompactionThreadItem"]
        #[serde(rename = "contextCompaction")]
        ContextCompaction { id: ::std::string::String },
    }
    #[doc = "`ThreadNameUpdatedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadNameUpdatedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"threadName\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadNameUpdatedNotification {
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(
            rename = "threadName",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub thread_name: ::std::option::Option<::std::string::String>,
    }
    #[doc = "EXPERIMENTAL - thread realtime audio chunk."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"EXPERIMENTAL - thread realtime audio chunk.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"data\","]
    #[doc = "    \"numChannels\","]
    #[doc = "    \"sampleRate\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"data\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"itemId\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"numChannels\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"uint16\","]
    #[doc = "      \"minimum\": 0.0"]
    #[doc = "    },"]
    #[doc = "    \"sampleRate\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"uint32\","]
    #[doc = "      \"minimum\": 0.0"]
    #[doc = "    },"]
    #[doc = "    \"samplesPerChannel\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"uint32\","]
    #[doc = "          \"minimum\": 0.0"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadRealtimeAudioChunk {
        pub data: ::std::string::String,
        #[serde(
            rename = "itemId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub item_id: ::std::option::Option<::std::string::String>,
        #[serde(rename = "numChannels")]
        pub num_channels: u16,
        #[serde(rename = "sampleRate")]
        pub sample_rate: u32,
        #[serde(
            rename = "samplesPerChannel",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub samples_per_channel: ::std::option::Option<u32>,
    }
    #[doc = "EXPERIMENTAL - emitted when thread realtime transport closes."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadRealtimeClosedNotification\","]
    #[doc = "  \"description\": \"EXPERIMENTAL - emitted when thread realtime transport closes.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"reason\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadRealtimeClosedNotification {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub reason: ::std::option::Option<::std::string::String>,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
    }
    #[doc = "EXPERIMENTAL - emitted when thread realtime encounters an error."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadRealtimeErrorNotification\","]
    #[doc = "  \"description\": \"EXPERIMENTAL - emitted when thread realtime encounters an error.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"message\","]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"message\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadRealtimeErrorNotification {
        pub message: ::std::string::String,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
    }
    #[doc = "EXPERIMENTAL - raw non-audio thread realtime item emitted by the backend."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadRealtimeItemAddedNotification\","]
    #[doc = "  \"description\": \"EXPERIMENTAL - raw non-audio thread realtime item emitted by the backend.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"item\","]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"item\": true,"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadRealtimeItemAddedNotification {
        pub item: ::serde_json::Value,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
    }
    #[doc = "EXPERIMENTAL - streamed output audio emitted by thread realtime."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadRealtimeOutputAudioDeltaNotification\","]
    #[doc = "  \"description\": \"EXPERIMENTAL - streamed output audio emitted by thread realtime.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"audio\","]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"audio\": {"]
    #[doc = "      \"$ref\": \"#/definitions/ThreadRealtimeAudioChunk\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadRealtimeOutputAudioDeltaNotification {
        pub audio: ThreadRealtimeAudioChunk,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
    }
    #[doc = "EXPERIMENTAL - emitted with the remote SDP for a WebRTC realtime session."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadRealtimeSdpNotification\","]
    #[doc = "  \"description\": \"EXPERIMENTAL - emitted with the remote SDP for a WebRTC realtime session.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"sdp\","]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"sdp\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadRealtimeSdpNotification {
        pub sdp: ::std::string::String,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
    }
    #[doc = "EXPERIMENTAL - emitted when thread realtime startup is accepted."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadRealtimeStartedNotification\","]
    #[doc = "  \"description\": \"EXPERIMENTAL - emitted when thread realtime startup is accepted.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"threadId\","]
    #[doc = "    \"version\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"realtimeSessionId\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"version\": {"]
    #[doc = "      \"$ref\": \"#/definitions/RealtimeConversationVersion\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadRealtimeStartedNotification {
        #[serde(
            rename = "realtimeSessionId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub realtime_session_id: ::std::option::Option<::std::string::String>,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        pub version: RealtimeConversationVersion,
    }
    #[doc = "EXPERIMENTAL - flat transcript delta emitted whenever realtime transcript text changes."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadRealtimeTranscriptDeltaNotification\","]
    #[doc = "  \"description\": \"EXPERIMENTAL - flat transcript delta emitted whenever realtime transcript text changes.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"delta\","]
    #[doc = "    \"role\","]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"delta\": {"]
    #[doc = "      \"description\": \"Live transcript delta from the realtime event.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"role\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadRealtimeTranscriptDeltaNotification {
        #[doc = "Live transcript delta from the realtime event."]
        pub delta: ::std::string::String,
        pub role: ::std::string::String,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
    }
    #[doc = "EXPERIMENTAL - final transcript text emitted when realtime completes a transcript part."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadRealtimeTranscriptDoneNotification\","]
    #[doc = "  \"description\": \"EXPERIMENTAL - final transcript text emitted when realtime completes a transcript part.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"role\","]
    #[doc = "    \"text\","]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"role\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"text\": {"]
    #[doc = "      \"description\": \"Final complete text for the transcript part.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadRealtimeTranscriptDoneNotification {
        pub role: ::std::string::String,
        #[doc = "Final complete text for the transcript part."]
        pub text: ::std::string::String,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
    }
    #[doc = "`ThreadSettings`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"approvalPolicy\","]
    #[doc = "    \"approvalsReviewer\","]
    #[doc = "    \"collaborationMode\","]
    #[doc = "    \"cwd\","]
    #[doc = "    \"model\","]
    #[doc = "    \"modelProvider\","]
    #[doc = "    \"sandboxPolicy\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"activePermissionProfile\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/ActivePermissionProfile\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"approvalPolicy\": {"]
    #[doc = "      \"$ref\": \"#/definitions/AskForApproval\""]
    #[doc = "    },"]
    #[doc = "    \"approvalsReviewer\": {"]
    #[doc = "      \"$ref\": \"#/definitions/ApprovalsReviewer\""]
    #[doc = "    },"]
    #[doc = "    \"collaborationMode\": {"]
    #[doc = "      \"$ref\": \"#/definitions/CollaborationMode\""]
    #[doc = "    },"]
    #[doc = "    \"cwd\": {"]
    #[doc = "      \"$ref\": \"#/definitions/AbsolutePathBuf\""]
    #[doc = "    },"]
    #[doc = "    \"effort\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/ReasoningEffort\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"model\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"modelProvider\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"personality\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/Personality\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"sandboxPolicy\": {"]
    #[doc = "      \"$ref\": \"#/definitions/SandboxPolicy\""]
    #[doc = "    },"]
    #[doc = "    \"serviceTier\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"summary\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/ReasoningSummary\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadSettings {
        #[serde(
            rename = "activePermissionProfile",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub active_permission_profile: ::std::option::Option<ActivePermissionProfile>,
        #[serde(rename = "approvalPolicy")]
        pub approval_policy: AskForApproval,
        #[serde(rename = "approvalsReviewer")]
        pub approvals_reviewer: ApprovalsReviewer,
        #[serde(rename = "collaborationMode")]
        pub collaboration_mode: CollaborationMode,
        pub cwd: AbsolutePathBuf,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub effort: ::std::option::Option<ReasoningEffort>,
        pub model: ::std::string::String,
        #[serde(rename = "modelProvider")]
        pub model_provider: ::std::string::String,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub personality: ::std::option::Option<Personality>,
        #[serde(rename = "sandboxPolicy")]
        pub sandbox_policy: SandboxPolicy,
        #[serde(
            rename = "serviceTier",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub service_tier: ::std::option::Option<::std::string::String>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub summary: ::std::option::Option<ReasoningSummary>,
    }
    #[doc = "`ThreadSettingsUpdatedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadSettingsUpdatedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"threadId\","]
    #[doc = "    \"threadSettings\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"threadSettings\": {"]
    #[doc = "      \"$ref\": \"#/definitions/ThreadSettings\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadSettingsUpdatedNotification {
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "threadSettings")]
        pub thread_settings: ThreadSettings,
    }
    #[doc = "`ThreadSource`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    #[serde(transparent)]
    pub struct ThreadSource(pub ::std::string::String);
    impl ::std::ops::Deref for ThreadSource {
        type Target = ::std::string::String;
        fn deref(&self) -> &::std::string::String {
            &self.0
        }
    }
    impl ::std::convert::From<ThreadSource> for ::std::string::String {
        fn from(value: ThreadSource) -> Self {
            value.0
        }
    }
    impl ::std::convert::From<::std::string::String> for ThreadSource {
        fn from(value: ::std::string::String) -> Self {
            Self(value)
        }
    }
    impl ::std::str::FromStr for ThreadSource {
        type Err = ::std::convert::Infallible;
        fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
            Ok(Self(value.to_string()))
        }
    }
    impl ::std::fmt::Display for ThreadSource {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            self.0.fmt(f)
        }
    }
    #[doc = "`ThreadStartedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadStartedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"thread\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"thread\": {"]
    #[doc = "      \"$ref\": \"#/definitions/Thread\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadStartedNotification {
        pub thread: Thread,
    }
    #[doc = "`ThreadStatus`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"NotLoadedThreadStatus\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"NotLoadedThreadStatusType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"notLoaded\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"IdleThreadStatus\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"IdleThreadStatusType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"idle\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SystemErrorThreadStatus\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"SystemErrorThreadStatusType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"systemError\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ActiveThreadStatus\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"activeFlags\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"activeFlags\": {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/ThreadActiveFlag\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ActiveThreadStatusType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"active\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type", content = "activeFlags")]
    pub enum ThreadStatus {
        #[serde(rename = "notLoaded")]
        NotLoaded,
        #[serde(rename = "idle")]
        Idle,
        #[serde(rename = "systemError")]
        SystemError,
        #[doc = "ActiveThreadStatus"]
        #[serde(rename = "active")]
        Active(::std::vec::Vec<ThreadActiveFlag>),
    }
    impl ::std::convert::From<::std::vec::Vec<ThreadActiveFlag>> for ThreadStatus {
        fn from(value: ::std::vec::Vec<ThreadActiveFlag>) -> Self {
            Self::Active(value)
        }
    }
    #[doc = "`ThreadStatusChangedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadStatusChangedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"status\","]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"status\": {"]
    #[doc = "      \"$ref\": \"#/definitions/ThreadStatus\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadStatusChangedNotification {
        pub status: ThreadStatus,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
    }
    #[doc = "`ThreadTokenUsage`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"last\","]
    #[doc = "    \"total\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"last\": {"]
    #[doc = "      \"$ref\": \"#/definitions/TokenUsageBreakdown\""]
    #[doc = "    },"]
    #[doc = "    \"modelContextWindow\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"int64\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"total\": {"]
    #[doc = "      \"$ref\": \"#/definitions/TokenUsageBreakdown\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadTokenUsage {
        pub last: TokenUsageBreakdown,
        #[serde(
            rename = "modelContextWindow",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[ts(type = "number | null")]
        pub model_context_window: ::std::option::Option<i64>,
        pub total: TokenUsageBreakdown,
    }
    #[doc = "`ThreadTokenUsageUpdatedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadTokenUsageUpdatedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"threadId\","]
    #[doc = "    \"tokenUsage\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"tokenUsage\": {"]
    #[doc = "      \"$ref\": \"#/definitions/ThreadTokenUsage\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadTokenUsageUpdatedNotification {
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "tokenUsage")]
        pub token_usage: ThreadTokenUsage,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`ThreadUnarchivedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ThreadUnarchivedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"threadId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ThreadUnarchivedNotification {
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
    }
    #[doc = "`TokenUsageBreakdown`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"cachedInputTokens\","]
    #[doc = "    \"inputTokens\","]
    #[doc = "    \"outputTokens\","]
    #[doc = "    \"reasoningOutputTokens\","]
    #[doc = "    \"totalTokens\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"cachedInputTokens\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"inputTokens\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"outputTokens\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"reasoningOutputTokens\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"totalTokens\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct TokenUsageBreakdown {
        #[serde(rename = "cachedInputTokens")]
        #[ts(type = "number")]
        pub cached_input_tokens: i64,
        #[serde(rename = "inputTokens")]
        #[ts(type = "number")]
        pub input_tokens: i64,
        #[serde(rename = "outputTokens")]
        #[ts(type = "number")]
        pub output_tokens: i64,
        #[serde(rename = "reasoningOutputTokens")]
        #[ts(type = "number")]
        pub reasoning_output_tokens: i64,
        #[serde(rename = "totalTokens")]
        #[ts(type = "number")]
        pub total_tokens: i64,
    }
    #[doc = "`Turn`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"id\","]
    #[doc = "    \"items\","]
    #[doc = "    \"status\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"completedAt\": {"]
    #[doc = "      \"description\": \"Unix timestamp (in seconds) when the turn completed.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Unix timestamp (in seconds) when the turn completed.\","]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"int64\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"durationMs\": {"]
    #[doc = "      \"description\": \"Duration between turn start and completion in milliseconds, if known.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Duration between turn start and completion in milliseconds, if known.\","]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"int64\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"error\": {"]
    #[doc = "      \"description\": \"Only populated when the Turn's status is failed.\","]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/TurnError\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"id\": {"]
    #[doc = "      \"description\": \"Identifier for this turn. Codex-generated turn IDs are UUIDv7.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"items\": {"]
    #[doc = "      \"description\": \"Thread items currently included in this turn payload.\","]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"$ref\": \"#/definitions/ThreadItem\""]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    \"itemsView\": {"]
    #[doc = "      \"description\": \"Describes how much of `items` has been loaded for this turn.\","]
    #[doc = "      \"default\": \"full\","]
    #[doc = "      \"allOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/TurnItemsView\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"startedAt\": {"]
    #[doc = "      \"description\": \"Unix timestamp (in seconds) when the turn started.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Unix timestamp (in seconds) when the turn started.\","]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"int64\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"status\": {"]
    #[doc = "      \"$ref\": \"#/definitions/TurnStatus\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct Turn {
        #[doc = "Unix timestamp (in seconds) when the turn completed."]
        #[serde(
            rename = "completedAt",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        #[ts(type = "number | null")]
        pub completed_at: ::std::option::Option<::std::option::Option<i64>>,
        #[doc = "Duration between turn start and completion in milliseconds, if known."]
        #[serde(
            rename = "durationMs",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        #[ts(type = "number | null")]
        pub duration_ms: ::std::option::Option<::std::option::Option<i64>>,
        #[doc = "Only populated when the Turn's status is failed."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub error: ::std::option::Option<::std::option::Option<TurnError>>,
        #[doc = "Identifier for this turn. Codex-generated turn IDs are UUIDv7."]
        pub id: ::std::string::String,
        #[doc = "Thread items currently included in this turn payload."]
        pub items: ::std::vec::Vec<ThreadItem>,
        #[doc = "Describes how much of `items` has been loaded for this turn."]
        #[serde(rename = "itemsView", default = "defaults::turn_items_view")]
        pub items_view: TurnItemsView,
        #[doc = "Unix timestamp (in seconds) when the turn started."]
        #[serde(
            rename = "startedAt",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        #[ts(type = "number | null")]
        pub started_at: ::std::option::Option<::std::option::Option<i64>>,
        pub status: TurnStatus,
    }
    #[doc = "`TurnCompletedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"TurnCompletedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turn\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turn\": {"]
    #[doc = "      \"$ref\": \"#/definitions/Turn\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct TurnCompletedNotification {
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        pub turn: Turn,
    }
    #[doc = "Notification that the turn-level unified diff has changed. Contains the latest aggregated diff across all file changes in the turn."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"TurnDiffUpdatedNotification\","]
    #[doc = "  \"description\": \"Notification that the turn-level unified diff has changed. Contains the latest aggregated diff across all file changes in the turn.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"diff\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"diff\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct TurnDiffUpdatedNotification {
        pub diff: ::std::string::String,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`TurnError`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"message\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"additionalDetails\": {"]
    #[doc = "      \"default\": null,"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"default\": null,"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"codexErrorInfo\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/CodexErrorInfo\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"message\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct TurnError {
        #[serde(
            rename = "additionalDetails",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub additional_details: ::std::option::Option<::std::option::Option<::std::string::String>>,
        #[serde(
            rename = "codexErrorInfo",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub codex_error_info: ::std::option::Option<::std::option::Option<CodexErrorInfo>>,
        pub message: ::std::string::String,
    }
    #[doc = "`TurnItemsView`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"description\": \"`items` was not loaded for this turn. The field is intentionally empty.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"notLoaded\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"description\": \"`items` contains only a display summary for this turn.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"summary\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"description\": \"`items` contains every ThreadItem available from persisted app-server history for this turn.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"full\""]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum TurnItemsView {
        #[doc = "`items` was not loaded for this turn. The field is intentionally empty."]
        #[serde(rename = "notLoaded")]
        NotLoaded,
        #[doc = "`items` contains only a display summary for this turn."]
        #[serde(rename = "summary")]
        Summary,
        #[doc = "`items` contains every ThreadItem available from persisted app-server history for this turn."]
        #[serde(rename = "full")]
        Full,
    }
    impl ::std::fmt::Display for TurnItemsView {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::NotLoaded => f.write_str("notLoaded"),
                Self::Summary => f.write_str("summary"),
                Self::Full => f.write_str("full"),
            }
        }
    }
    impl ::std::str::FromStr for TurnItemsView {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "notLoaded" => Ok(Self::NotLoaded),
                "summary" => Ok(Self::Summary),
                "full" => Ok(Self::Full),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for TurnItemsView {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for TurnItemsView {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for TurnItemsView {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`TurnModerationMetadataNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"TurnModerationMetadataNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"metadata\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"metadata\": true,"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct TurnModerationMetadataNotification {
        pub metadata: ::serde_json::Value,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`TurnPlanStep`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"status\","]
    #[doc = "    \"step\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"status\": {"]
    #[doc = "      \"$ref\": \"#/definitions/TurnPlanStepStatus\""]
    #[doc = "    },"]
    #[doc = "    \"step\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct TurnPlanStep {
        pub status: TurnPlanStepStatus,
        pub step: ::std::string::String,
    }
    #[doc = "`TurnPlanStepStatus`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"pending\","]
    #[doc = "    \"inProgress\","]
    #[doc = "    \"completed\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum TurnPlanStepStatus {
        #[serde(rename = "pending")]
        Pending,
        #[serde(rename = "inProgress")]
        InProgress,
        #[serde(rename = "completed")]
        Completed,
    }
    impl ::std::fmt::Display for TurnPlanStepStatus {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Pending => f.write_str("pending"),
                Self::InProgress => f.write_str("inProgress"),
                Self::Completed => f.write_str("completed"),
            }
        }
    }
    impl ::std::str::FromStr for TurnPlanStepStatus {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "pending" => Ok(Self::Pending),
                "inProgress" => Ok(Self::InProgress),
                "completed" => Ok(Self::Completed),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for TurnPlanStepStatus {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for TurnPlanStepStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for TurnPlanStepStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`TurnPlanUpdatedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"TurnPlanUpdatedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"plan\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"explanation\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"plan\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"$ref\": \"#/definitions/TurnPlanStep\""]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct TurnPlanUpdatedNotification {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub explanation: ::std::option::Option<::std::option::Option<::std::string::String>>,
        pub plan: ::std::vec::Vec<TurnPlanStep>,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`TurnStartedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"TurnStartedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turn\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turn\": {"]
    #[doc = "      \"$ref\": \"#/definitions/Turn\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct TurnStartedNotification {
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        pub turn: Turn,
    }
    #[doc = "`TurnStatus`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"completed\","]
    #[doc = "    \"interrupted\","]
    #[doc = "    \"failed\","]
    #[doc = "    \"inProgress\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum TurnStatus {
        #[serde(rename = "completed")]
        Completed,
        #[serde(rename = "interrupted")]
        Interrupted,
        #[serde(rename = "failed")]
        Failed,
        #[serde(rename = "inProgress")]
        InProgress,
    }
    impl ::std::fmt::Display for TurnStatus {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Completed => f.write_str("completed"),
                Self::Interrupted => f.write_str("interrupted"),
                Self::Failed => f.write_str("failed"),
                Self::InProgress => f.write_str("inProgress"),
            }
        }
    }
    impl ::std::str::FromStr for TurnStatus {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "completed" => Ok(Self::Completed),
                "interrupted" => Ok(Self::Interrupted),
                "failed" => Ok(Self::Failed),
                "inProgress" => Ok(Self::InProgress),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for TurnStatus {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for TurnStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for TurnStatus {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`UserInput`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"TextUserInput\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"text\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"text\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"text_elements\": {"]
    #[doc = "          \"description\": \"UI-defined spans within `text` used to render or persist special elements.\","]
    #[doc = "          \"default\": [],"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/TextElement\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"TextUserInputType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"text\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ImageUserInput\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\","]
    #[doc = "        \"url\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"detail\": {"]
    #[doc = "          \"default\": null,"]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/ImageDetail\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ImageUserInputType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"image\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"url\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"LocalImageUserInput\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"path\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"detail\": {"]
    #[doc = "          \"default\": null,"]
    #[doc = "          \"anyOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"$ref\": \"#/definitions/ImageDetail\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"LocalImageUserInputType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"localImage\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SkillUserInput\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"name\","]
    #[doc = "        \"path\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"name\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"SkillUserInputType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"skill\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"MentionUserInput\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"name\","]
    #[doc = "        \"path\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"name\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"MentionUserInputType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"mention\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type")]
    pub enum UserInput {
        #[doc = "TextUserInput"]
        #[serde(rename = "text")]
        Text {
            text: ::std::string::String,
            #[doc = "UI-defined spans within `text` used to render or persist special elements."]
            #[serde(default)]
            text_elements: ::std::vec::Vec<TextElement>,
        },
        #[doc = "ImageUserInput"]
        #[serde(rename = "image")]
        Image {
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            detail: ::std::option::Option<::std::option::Option<ImageDetail>>,
            url: ::std::string::String,
        },
        #[doc = "LocalImageUserInput"]
        #[serde(rename = "localImage")]
        LocalImage {
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            detail: ::std::option::Option<::std::option::Option<ImageDetail>>,
            path: ::std::string::String,
        },
        #[doc = "SkillUserInput"]
        #[serde(rename = "skill")]
        Skill {
            name: ::std::string::String,
            path: ::std::string::String,
        },
        #[doc = "MentionUserInput"]
        #[serde(rename = "mention")]
        Mention {
            name: ::std::string::String,
            path: ::std::string::String,
        },
    }
    #[doc = "`WarningNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"WarningNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"message\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"message\": {"]
    #[doc = "      \"description\": \"Concise warning message for the user.\","]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"description\": \"Optional thread target when the warning applies to a specific thread.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Optional thread target when the warning applies to a specific thread.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct WarningNotification {
        #[doc = "Concise warning message for the user."]
        pub message: ::std::string::String,
        #[doc = "Optional thread target when the warning applies to a specific thread."]
        #[serde(
            rename = "threadId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
        pub thread_id: ::std::option::Option<::std::option::Option<::std::string::String>>,
    }
    #[doc = "`WebSearchAction`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SearchWebSearchAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"queries\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"array\","]
    #[doc = "              \"items\": {"]
    #[doc = "                \"type\": \"string\""]
    #[doc = "              }"]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"query\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"SearchWebSearchActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"search\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"OpenPageWebSearchAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"OpenPageWebSearchActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"openPage\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"url\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"FindInPageWebSearchAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"pattern\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"FindInPageWebSearchActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"findInPage\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"url\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"OtherWebSearchAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"OtherWebSearchActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"other\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type")]
    pub enum WebSearchAction {
        #[doc = "SearchWebSearchAction"]
        #[serde(rename = "search")]
        Search {
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            queries: ::std::option::Option<
                ::std::option::Option<::std::vec::Vec<::std::string::String>>,
            >,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            query: ::std::option::Option<::std::option::Option<::std::string::String>>,
        },
        #[doc = "OpenPageWebSearchAction"]
        #[serde(rename = "openPage")]
        OpenPage {
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            url: ::std::option::Option<::std::option::Option<::std::string::String>>,
        },
        #[doc = "FindInPageWebSearchAction"]
        #[serde(rename = "findInPage")]
        FindInPage {
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            pattern: ::std::option::Option<::std::option::Option<::std::string::String>>,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            #[serde(deserialize_with = "super::deserialize_optional_explicit_null")]
            url: ::std::option::Option<::std::option::Option<::std::string::String>>,
        },
        #[serde(rename = "other")]
        Other,
    }
    #[doc = "`WindowsSandboxSetupCompletedNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"WindowsSandboxSetupCompletedNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"mode\","]
    #[doc = "    \"success\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"error\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"mode\": {"]
    #[doc = "      \"$ref\": \"#/definitions/WindowsSandboxSetupMode\""]
    #[doc = "    },"]
    #[doc = "    \"success\": {"]
    #[doc = "      \"type\": \"boolean\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct WindowsSandboxSetupCompletedNotification {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub error: ::std::option::Option<::std::string::String>,
        pub mode: WindowsSandboxSetupMode,
        pub success: bool,
    }
    #[doc = "`WindowsSandboxSetupMode`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"elevated\","]
    #[doc = "    \"unelevated\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum WindowsSandboxSetupMode {
        #[serde(rename = "elevated")]
        Elevated,
        #[serde(rename = "unelevated")]
        Unelevated,
    }
    impl ::std::fmt::Display for WindowsSandboxSetupMode {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Elevated => f.write_str("elevated"),
                Self::Unelevated => f.write_str("unelevated"),
            }
        }
    }
    impl ::std::str::FromStr for WindowsSandboxSetupMode {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "elevated" => Ok(Self::Elevated),
                "unelevated" => Ok(Self::Unelevated),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for WindowsSandboxSetupMode {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for WindowsSandboxSetupMode {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for WindowsSandboxSetupMode {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`WindowsWorldWritableWarningNotification`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"WindowsWorldWritableWarningNotification\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"extraCount\","]
    #[doc = "    \"failedScan\","]
    #[doc = "    \"samplePaths\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"extraCount\": {"]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"uint\","]
    #[doc = "      \"minimum\": 0.0"]
    #[doc = "    },"]
    #[doc = "    \"failedScan\": {"]
    #[doc = "      \"type\": \"boolean\""]
    #[doc = "    },"]
    #[doc = "    \"samplePaths\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"type\": \"string\""]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"$schema\": \"http://json-schema.org/draft-07/schema#\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct WindowsWorldWritableWarningNotification {
        #[serde(rename = "extraCount")]
        pub extra_count: u32,
        #[serde(rename = "failedScan")]
        pub failed_scan: bool,
        #[serde(rename = "samplePaths")]
        pub sample_paths: ::std::vec::Vec<::std::string::String>,
    }
    #[doc = r" Generation of default values for serde."]
    pub mod defaults {
        pub(super) fn default_bool<const V: bool>() -> bool {
            V
        }
        pub(super) fn hook_run_summary_source() -> super::HookSource {
            super::HookSource::Unknown
        }
        pub(super) fn sandbox_policy_external_sandbox_network_access() -> super::NetworkAccess {
            super::NetworkAccess::Restricted
        }
        pub(super) fn thread_item_command_execution_source() -> super::CommandExecutionSource {
            super::CommandExecutionSource::Agent
        }
        pub(super) fn turn_items_view() -> super::TurnItemsView {
            super::TurnItemsView::Full
        }
    }
}
pub mod command_execution_request_approval_params {
    #[doc = r" Error types."]
    pub mod error {
        #[doc = r" Error from a `TryFrom` or `FromStr` implementation."]
        pub struct ConversionError(::std::borrow::Cow<'static, str>);
        impl ::std::error::Error for ConversionError {}
        impl ::std::fmt::Display for ConversionError {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                ::std::fmt::Display::fmt(&self.0, f)
            }
        }
        impl ::std::fmt::Debug for ConversionError {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                ::std::fmt::Debug::fmt(&self.0, f)
            }
        }
        impl From<&'static str> for ConversionError {
            fn from(value: &'static str) -> Self {
                Self(value.into())
            }
        }
        impl From<String> for ConversionError {
            fn from(value: String) -> Self {
                Self(value.into())
            }
        }
    }
    #[doc = "A path that is guaranteed to be absolute and normalized (though it is not guaranteed to be canonicalized or exist on the filesystem).\n\nIMPORTANT: When deserializing an `AbsolutePathBuf`, a base path must be set using [AbsolutePathBufGuard::new]. If no base path is set, the deserialization will fail unless the path being deserialized is already absolute."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"A path that is guaranteed to be absolute and normalized (though it is not guaranteed to be canonicalized or exist on the filesystem).\\n\\nIMPORTANT: When deserializing an `AbsolutePathBuf`, a base path must be set using [AbsolutePathBufGuard::new]. If no base path is set, the deserialization will fail unless the path being deserialized is already absolute.\","]
    #[doc = "  \"type\": \"string\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    #[serde(transparent)]
    pub struct AbsolutePathBuf(pub ::std::string::String);
    impl ::std::ops::Deref for AbsolutePathBuf {
        type Target = ::std::string::String;
        fn deref(&self) -> &::std::string::String {
            &self.0
        }
    }
    impl ::std::convert::From<AbsolutePathBuf> for ::std::string::String {
        fn from(value: AbsolutePathBuf) -> Self {
            value.0
        }
    }
    impl ::std::convert::From<::std::string::String> for AbsolutePathBuf {
        fn from(value: ::std::string::String) -> Self {
            Self(value)
        }
    }
    impl ::std::str::FromStr for AbsolutePathBuf {
        type Err = ::std::convert::Infallible;
        fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
            Ok(Self(value.to_string()))
        }
    }
    impl ::std::fmt::Display for AbsolutePathBuf {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            self.0.fmt(f)
        }
    }
    #[doc = "`AdditionalFileSystemPermissions`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"entries\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/FileSystemSandboxEntry\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"globScanMaxDepth\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"uint\","]
    #[doc = "          \"minimum\": 1.0"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"read\": {"]
    #[doc = "      \"description\": \"This will be removed in favor of `entries`.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"This will be removed in favor of `entries`.\","]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/LegacyAppPathString\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"write\": {"]
    #[doc = "      \"description\": \"This will be removed in favor of `entries`.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"This will be removed in favor of `entries`.\","]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/LegacyAppPathString\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct AdditionalFileSystemPermissions {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub entries: ::std::option::Option<::std::vec::Vec<FileSystemSandboxEntry>>,
        #[serde(
            rename = "globScanMaxDepth",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub glob_scan_max_depth: ::std::option::Option<::std::num::NonZeroU32>,
        #[doc = "This will be removed in favor of `entries`."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub read: ::std::option::Option<::std::vec::Vec<LegacyAppPathString>>,
        #[doc = "This will be removed in favor of `entries`."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub write: ::std::option::Option<::std::vec::Vec<LegacyAppPathString>>,
    }
    impl ::std::default::Default for AdditionalFileSystemPermissions {
        fn default() -> Self {
            Self {
                entries: Default::default(),
                glob_scan_max_depth: Default::default(),
                read: Default::default(),
                write: Default::default(),
            }
        }
    }
    #[doc = "`AdditionalNetworkPermissions`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"enabled\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"boolean\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct AdditionalNetworkPermissions {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub enabled: ::std::option::Option<bool>,
    }
    impl ::std::default::Default for AdditionalNetworkPermissions {
        fn default() -> Self {
            Self {
                enabled: Default::default(),
            }
        }
    }
    #[doc = "`AdditionalPermissionProfile`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"fileSystem\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/AdditionalFileSystemPermissions\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"network\": {"]
    #[doc = "      \"description\": \"Partial overlay used for per-command permission requests.\","]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/AdditionalNetworkPermissions\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct AdditionalPermissionProfile {
        #[serde(
            rename = "fileSystem",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub file_system: ::std::option::Option<AdditionalFileSystemPermissions>,
        #[doc = "Partial overlay used for per-command permission requests."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub network: ::std::option::Option<AdditionalNetworkPermissions>,
    }
    impl ::std::default::Default for AdditionalPermissionProfile {
        fn default() -> Self {
            Self {
                file_system: Default::default(),
                network: Default::default(),
            }
        }
    }
    #[doc = "`CommandAction`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ReadCommandAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"command\","]
    #[doc = "        \"name\","]
    #[doc = "        \"path\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"command\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"name\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"$ref\": \"#/definitions/AbsolutePathBuf\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ReadCommandActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"read\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ListFilesCommandAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"command\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"command\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"ListFilesCommandActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"listFiles\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SearchCommandAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"command\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"command\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"query\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"SearchCommandActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"search\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"UnknownCommandAction\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"command\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"command\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"UnknownCommandActionType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"unknown\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type")]
    pub enum CommandAction {
        #[doc = "ReadCommandAction"]
        #[serde(rename = "read")]
        Read {
            command: ::std::string::String,
            name: ::std::string::String,
            path: AbsolutePathBuf,
        },
        #[doc = "ListFilesCommandAction"]
        #[serde(rename = "listFiles")]
        ListFiles {
            command: ::std::string::String,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            path: ::std::option::Option<::std::string::String>,
        },
        #[doc = "SearchCommandAction"]
        #[serde(rename = "search")]
        Search {
            command: ::std::string::String,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            path: ::std::option::Option<::std::string::String>,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            query: ::std::option::Option<::std::string::String>,
        },
        #[doc = "UnknownCommandAction"]
        #[serde(rename = "unknown")]
        Unknown { command: ::std::string::String },
    }
    #[doc = "`CommandExecutionApprovalDecision`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"description\": \"User approved the command.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"accept\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"description\": \"User approved the command and future prompts in the same session-scoped approval cache should run without prompting.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"acceptForSession\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"AcceptWithExecpolicyAmendmentCommandExecutionApprovalDecision\","]
    #[doc = "      \"description\": \"User approved the command, and wants to apply the proposed execpolicy amendment so future matching commands can run without prompting.\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"acceptWithExecpolicyAmendment\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"acceptWithExecpolicyAmendment\": {"]
    #[doc = "          \"type\": \"object\","]
    #[doc = "          \"required\": ["]
    #[doc = "            \"execpolicy_amendment\""]
    #[doc = "          ],"]
    #[doc = "          \"properties\": {"]
    #[doc = "            \"execpolicy_amendment\": {"]
    #[doc = "              \"type\": \"array\","]
    #[doc = "              \"items\": {"]
    #[doc = "                \"type\": \"string\""]
    #[doc = "              }"]
    #[doc = "            }"]
    #[doc = "          }"]
    #[doc = "        }"]
    #[doc = "      },"]
    #[doc = "      \"additionalProperties\": false"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"ApplyNetworkPolicyAmendmentCommandExecutionApprovalDecision\","]
    #[doc = "      \"description\": \"User chose a persistent network policy rule (allow/deny) for this host.\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"applyNetworkPolicyAmendment\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"applyNetworkPolicyAmendment\": {"]
    #[doc = "          \"type\": \"object\","]
    #[doc = "          \"required\": ["]
    #[doc = "            \"network_policy_amendment\""]
    #[doc = "          ],"]
    #[doc = "          \"properties\": {"]
    #[doc = "            \"network_policy_amendment\": {"]
    #[doc = "              \"$ref\": \"#/definitions/NetworkPolicyAmendment\""]
    #[doc = "            }"]
    #[doc = "          }"]
    #[doc = "        }"]
    #[doc = "      },"]
    #[doc = "      \"additionalProperties\": false"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"description\": \"User denied the command. The agent will continue the turn.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"decline\""]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"description\": \"User denied the command. The turn will also be immediately interrupted.\","]
    #[doc = "      \"type\": \"string\","]
    #[doc = "      \"enum\": ["]
    #[doc = "        \"cancel\""]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub enum CommandExecutionApprovalDecision {
        #[doc = "User approved the command."]
        #[serde(rename = "accept")]
        Accept,
        #[doc = "User approved the command and future prompts in the same session-scoped approval cache should run without prompting."]
        #[serde(rename = "acceptForSession")]
        AcceptForSession,
        #[doc = "User approved the command, and wants to apply the proposed execpolicy amendment so future matching commands can run without prompting."]
        #[serde(rename = "acceptWithExecpolicyAmendment")]
        AcceptWithExecpolicyAmendment {
            execpolicy_amendment: ::std::vec::Vec<::std::string::String>,
        },
        #[doc = "User chose a persistent network policy rule (allow/deny) for this host."]
        #[serde(rename = "applyNetworkPolicyAmendment")]
        ApplyNetworkPolicyAmendment {
            network_policy_amendment: NetworkPolicyAmendment,
        },
        #[doc = "User denied the command. The agent will continue the turn."]
        #[serde(rename = "decline")]
        Decline,
        #[doc = "User denied the command. The turn will also be immediately interrupted."]
        #[serde(rename = "cancel")]
        Cancel,
    }
    #[doc = "`CommandExecutionRequestApprovalParams`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"CommandExecutionRequestApprovalParams\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"itemId\","]
    #[doc = "    \"startedAtMs\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"approvalId\": {"]
    #[doc = "      \"description\": \"Unique identifier for this specific approval callback.\\n\\nFor regular shell/unified_exec approvals, this is null.\\n\\nFor zsh-exec-bridge subcommand approvals, multiple callbacks can belong to one parent `itemId`, so `approvalId` is a distinct opaque callback id (a UUID) used to disambiguate routing.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Unique identifier for this specific approval callback.\\n\\nFor regular shell/unified_exec approvals, this is null.\\n\\nFor zsh-exec-bridge subcommand approvals, multiple callbacks can belong to one parent `itemId`, so `approvalId` is a distinct opaque callback id (a UUID) used to disambiguate routing.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"command\": {"]
    #[doc = "      \"description\": \"The command to be executed.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"The command to be executed.\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"commandActions\": {"]
    #[doc = "      \"description\": \"Best-effort parsed command actions for friendly display.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Best-effort parsed command actions for friendly display.\","]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/CommandAction\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"cwd\": {"]
    #[doc = "      \"description\": \"The command's working directory.\","]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/LegacyAppPathString\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"environmentId\": {"]
    #[doc = "      \"description\": \"Environment in which the command will run.\","]
    #[doc = "      \"default\": null,"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Environment in which the command will run.\","]
    #[doc = "          \"default\": null,"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"itemId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"networkApprovalContext\": {"]
    #[doc = "      \"description\": \"Optional context for a managed-network approval prompt.\","]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/NetworkApprovalContext\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"proposedExecpolicyAmendment\": {"]
    #[doc = "      \"description\": \"Optional proposed execpolicy amendment to allow similar commands without prompting.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Optional proposed execpolicy amendment to allow similar commands without prompting.\","]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"type\": \"string\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"proposedNetworkPolicyAmendments\": {"]
    #[doc = "      \"description\": \"Optional proposed network policy amendments (allow/deny host) for future requests.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Optional proposed network policy amendments (allow/deny host) for future requests.\","]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/NetworkPolicyAmendment\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"reason\": {"]
    #[doc = "      \"description\": \"Optional explanatory reason (e.g. request for network access).\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Optional explanatory reason (e.g. request for network access).\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"startedAtMs\": {"]
    #[doc = "      \"description\": \"Unix timestamp (in milliseconds) when this approval request started.\","]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct CommandExecutionRequestApprovalParams {
        #[doc = "Unique identifier for this specific approval callback.\n\nFor regular shell/unified_exec approvals, this is null.\n\nFor zsh-exec-bridge subcommand approvals, multiple callbacks can belong to one parent `itemId`, so `approvalId` is a distinct opaque callback id (a UUID) used to disambiguate routing."]
        #[serde(
            rename = "approvalId",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub approval_id: ::std::option::Option<::std::string::String>,
        #[doc = "The command to be executed."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub command: ::std::option::Option<::std::string::String>,
        #[doc = "Best-effort parsed command actions for friendly display."]
        #[serde(
            rename = "commandActions",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub command_actions: ::std::option::Option<::std::vec::Vec<CommandAction>>,
        #[doc = "The command's working directory."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub cwd: ::std::option::Option<LegacyAppPathString>,
        #[doc = "Environment in which the command will run."]
        #[serde(rename = "environmentId", default)]
        pub environment_id: ::std::option::Option<::std::string::String>,
        #[serde(rename = "itemId")]
        pub item_id: ::std::string::String,
        #[doc = "Optional context for a managed-network approval prompt."]
        #[serde(
            rename = "networkApprovalContext",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub network_approval_context: ::std::option::Option<NetworkApprovalContext>,
        #[doc = "Optional proposed execpolicy amendment to allow similar commands without prompting."]
        #[serde(
            rename = "proposedExecpolicyAmendment",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub proposed_execpolicy_amendment:
            ::std::option::Option<::std::vec::Vec<::std::string::String>>,
        #[doc = "Optional proposed network policy amendments (allow/deny host) for future requests."]
        #[serde(
            rename = "proposedNetworkPolicyAmendments",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub proposed_network_policy_amendments:
            ::std::option::Option<::std::vec::Vec<NetworkPolicyAmendment>>,
        #[doc = "Optional explanatory reason (e.g. request for network access)."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub reason: ::std::option::Option<::std::string::String>,
        #[doc = "Unix timestamp (in milliseconds) when this approval request started."]
        #[serde(rename = "startedAtMs")]
        #[ts(type = "number")]
        pub started_at_ms: i64,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`FileSystemAccessMode`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"read\","]
    #[doc = "    \"write\","]
    #[doc = "    \"deny\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum FileSystemAccessMode {
        #[serde(rename = "read")]
        Read,
        #[serde(rename = "write")]
        Write,
        #[serde(rename = "deny")]
        Deny,
    }
    impl ::std::fmt::Display for FileSystemAccessMode {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Read => f.write_str("read"),
                Self::Write => f.write_str("write"),
                Self::Deny => f.write_str("deny"),
            }
        }
    }
    impl ::std::str::FromStr for FileSystemAccessMode {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "read" => Ok(Self::Read),
                "write" => Ok(Self::Write),
                "deny" => Ok(Self::Deny),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for FileSystemAccessMode {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for FileSystemAccessMode {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for FileSystemAccessMode {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`FileSystemPath`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"PathFileSystemPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"path\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"$ref\": \"#/definitions/LegacyAppPathString\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"PathFileSystemPathType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"path\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"GlobPatternFileSystemPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"pattern\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"pattern\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"GlobPatternFileSystemPathType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"glob_pattern\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SpecialFileSystemPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\","]
    #[doc = "        \"value\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"SpecialFileSystemPathType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"special\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"value\": {"]
    #[doc = "          \"$ref\": \"#/definitions/FileSystemSpecialPath\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type")]
    pub enum FileSystemPath {
        #[doc = "PathFileSystemPath"]
        #[serde(rename = "path")]
        Path { path: LegacyAppPathString },
        #[doc = "GlobPatternFileSystemPath"]
        #[serde(rename = "glob_pattern")]
        GlobPattern { pattern: ::std::string::String },
        #[doc = "SpecialFileSystemPath"]
        #[serde(rename = "special")]
        Special { value: FileSystemSpecialPath },
    }
    #[doc = "`FileSystemSandboxEntry`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"access\","]
    #[doc = "    \"path\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"access\": {"]
    #[doc = "      \"$ref\": \"#/definitions/FileSystemAccessMode\""]
    #[doc = "    },"]
    #[doc = "    \"path\": {"]
    #[doc = "      \"$ref\": \"#/definitions/FileSystemPath\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct FileSystemSandboxEntry {
        pub access: FileSystemAccessMode,
        pub path: FileSystemPath,
    }
    #[doc = "`FileSystemSpecialPath`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"RootFileSystemSpecialPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"kind\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"root\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"MinimalFileSystemSpecialPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"kind\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"minimal\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"KindFileSystemSpecialPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"kind\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"project_roots\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"subpath\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"TmpdirFileSystemSpecialPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"kind\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"tmpdir\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SlashTmpFileSystemSpecialPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"kind\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"slash_tmp\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"kind\","]
    #[doc = "        \"path\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"unknown\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"subpath\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "kind")]
    pub enum FileSystemSpecialPath {
        #[serde(rename = "root")]
        Root,
        #[serde(rename = "minimal")]
        Minimal,
        #[doc = "KindFileSystemSpecialPath"]
        #[serde(rename = "project_roots")]
        ProjectRoots {
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            subpath: ::std::option::Option<::std::string::String>,
        },
        #[serde(rename = "tmpdir")]
        Tmpdir,
        #[serde(rename = "slash_tmp")]
        SlashTmp,
        #[serde(rename = "unknown")]
        Unknown {
            path: ::std::string::String,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            subpath: ::std::option::Option<::std::string::String>,
        },
    }
    #[doc = "`LegacyAppPathString`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    #[serde(transparent)]
    pub struct LegacyAppPathString(pub ::std::string::String);
    impl ::std::ops::Deref for LegacyAppPathString {
        type Target = ::std::string::String;
        fn deref(&self) -> &::std::string::String {
            &self.0
        }
    }
    impl ::std::convert::From<LegacyAppPathString> for ::std::string::String {
        fn from(value: LegacyAppPathString) -> Self {
            value.0
        }
    }
    impl ::std::convert::From<::std::string::String> for LegacyAppPathString {
        fn from(value: ::std::string::String) -> Self {
            Self(value)
        }
    }
    impl ::std::str::FromStr for LegacyAppPathString {
        type Err = ::std::convert::Infallible;
        fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
            Ok(Self(value.to_string()))
        }
    }
    impl ::std::fmt::Display for LegacyAppPathString {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            self.0.fmt(f)
        }
    }
    #[doc = "`NetworkApprovalContext`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"host\","]
    #[doc = "    \"protocol\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"host\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"protocol\": {"]
    #[doc = "      \"$ref\": \"#/definitions/NetworkApprovalProtocol\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct NetworkApprovalContext {
        pub host: ::std::string::String,
        pub protocol: NetworkApprovalProtocol,
    }
    #[doc = "`NetworkApprovalProtocol`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"http\","]
    #[doc = "    \"https\","]
    #[doc = "    \"socks5Tcp\","]
    #[doc = "    \"socks5Udp\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum NetworkApprovalProtocol {
        #[serde(rename = "http")]
        Http,
        #[serde(rename = "https")]
        Https,
        #[serde(rename = "socks5Tcp")]
        Socks5Tcp,
        #[serde(rename = "socks5Udp")]
        Socks5Udp,
    }
    impl ::std::fmt::Display for NetworkApprovalProtocol {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Http => f.write_str("http"),
                Self::Https => f.write_str("https"),
                Self::Socks5Tcp => f.write_str("socks5Tcp"),
                Self::Socks5Udp => f.write_str("socks5Udp"),
            }
        }
    }
    impl ::std::str::FromStr for NetworkApprovalProtocol {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "http" => Ok(Self::Http),
                "https" => Ok(Self::Https),
                "socks5Tcp" => Ok(Self::Socks5Tcp),
                "socks5Udp" => Ok(Self::Socks5Udp),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for NetworkApprovalProtocol {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for NetworkApprovalProtocol {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for NetworkApprovalProtocol {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`NetworkPolicyAmendment`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"action\","]
    #[doc = "    \"host\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"action\": {"]
    #[doc = "      \"$ref\": \"#/definitions/NetworkPolicyRuleAction\""]
    #[doc = "    },"]
    #[doc = "    \"host\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct NetworkPolicyAmendment {
        pub action: NetworkPolicyRuleAction,
        pub host: ::std::string::String,
    }
    #[doc = "`NetworkPolicyRuleAction`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"allow\","]
    #[doc = "    \"deny\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum NetworkPolicyRuleAction {
        #[serde(rename = "allow")]
        Allow,
        #[serde(rename = "deny")]
        Deny,
    }
    impl ::std::fmt::Display for NetworkPolicyRuleAction {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Allow => f.write_str("allow"),
                Self::Deny => f.write_str("deny"),
            }
        }
    }
    impl ::std::str::FromStr for NetworkPolicyRuleAction {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "allow" => Ok(Self::Allow),
                "deny" => Ok(Self::Deny),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for NetworkPolicyRuleAction {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for NetworkPolicyRuleAction {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for NetworkPolicyRuleAction {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
}
pub mod file_change_request_approval_params {
    #[doc = r" Error types."]
    pub mod error {
        #[doc = r" Error from a `TryFrom` or `FromStr` implementation."]
        pub struct ConversionError(::std::borrow::Cow<'static, str>);
        impl ::std::error::Error for ConversionError {}
        impl ::std::fmt::Display for ConversionError {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                ::std::fmt::Display::fmt(&self.0, f)
            }
        }
        impl ::std::fmt::Debug for ConversionError {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                ::std::fmt::Debug::fmt(&self.0, f)
            }
        }
        impl From<&'static str> for ConversionError {
            fn from(value: &'static str) -> Self {
                Self(value.into())
            }
        }
        impl From<String> for ConversionError {
            fn from(value: String) -> Self {
                Self(value.into())
            }
        }
    }
    #[doc = "`FileChangeRequestApprovalParams`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"FileChangeRequestApprovalParams\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"itemId\","]
    #[doc = "    \"startedAtMs\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"grantRoot\": {"]
    #[doc = "      \"description\": \"[UNSTABLE] When set, the agent is asking the user to allow writes under this root for the remainder of the session (unclear if this is honored today).\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"[UNSTABLE] When set, the agent is asking the user to allow writes under this root for the remainder of the session (unclear if this is honored today).\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"itemId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"reason\": {"]
    #[doc = "      \"description\": \"Optional explanatory reason (e.g. request for extra write access).\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"Optional explanatory reason (e.g. request for extra write access).\","]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"startedAtMs\": {"]
    #[doc = "      \"description\": \"Unix timestamp (in milliseconds) when this approval request started.\","]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct FileChangeRequestApprovalParams {
        #[doc = "[UNSTABLE] When set, the agent is asking the user to allow writes under this root for the remainder of the session (unclear if this is honored today)."]
        #[serde(rename = "grantRoot", default)]
        pub grant_root: ::std::option::Option<::std::string::String>,
        #[serde(rename = "itemId")]
        pub item_id: ::std::string::String,
        #[doc = "Optional explanatory reason (e.g. request for extra write access)."]
        #[serde(default)]
        pub reason: ::std::option::Option<::std::string::String>,
        #[doc = "Unix timestamp (in milliseconds) when this approval request started."]
        #[serde(rename = "startedAtMs")]
        #[ts(type = "number")]
        pub started_at_ms: i64,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
}
pub mod permissions_request_approval_params {
    #[doc = r" Error types."]
    pub mod error {
        #[doc = r" Error from a `TryFrom` or `FromStr` implementation."]
        pub struct ConversionError(::std::borrow::Cow<'static, str>);
        impl ::std::error::Error for ConversionError {}
        impl ::std::fmt::Display for ConversionError {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                ::std::fmt::Display::fmt(&self.0, f)
            }
        }
        impl ::std::fmt::Debug for ConversionError {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                ::std::fmt::Debug::fmt(&self.0, f)
            }
        }
        impl From<&'static str> for ConversionError {
            fn from(value: &'static str) -> Self {
                Self(value.into())
            }
        }
        impl From<String> for ConversionError {
            fn from(value: String) -> Self {
                Self(value.into())
            }
        }
    }
    #[doc = "A path that is guaranteed to be absolute and normalized (though it is not guaranteed to be canonicalized or exist on the filesystem).\n\nIMPORTANT: When deserializing an `AbsolutePathBuf`, a base path must be set using [AbsolutePathBufGuard::new]. If no base path is set, the deserialization will fail unless the path being deserialized is already absolute."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"A path that is guaranteed to be absolute and normalized (though it is not guaranteed to be canonicalized or exist on the filesystem).\\n\\nIMPORTANT: When deserializing an `AbsolutePathBuf`, a base path must be set using [AbsolutePathBufGuard::new]. If no base path is set, the deserialization will fail unless the path being deserialized is already absolute.\","]
    #[doc = "  \"type\": \"string\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    #[serde(transparent)]
    pub struct AbsolutePathBuf(pub ::std::string::String);
    impl ::std::ops::Deref for AbsolutePathBuf {
        type Target = ::std::string::String;
        fn deref(&self) -> &::std::string::String {
            &self.0
        }
    }
    impl ::std::convert::From<AbsolutePathBuf> for ::std::string::String {
        fn from(value: AbsolutePathBuf) -> Self {
            value.0
        }
    }
    impl ::std::convert::From<::std::string::String> for AbsolutePathBuf {
        fn from(value: ::std::string::String) -> Self {
            Self(value)
        }
    }
    impl ::std::str::FromStr for AbsolutePathBuf {
        type Err = ::std::convert::Infallible;
        fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
            Ok(Self(value.to_string()))
        }
    }
    impl ::std::fmt::Display for AbsolutePathBuf {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            self.0.fmt(f)
        }
    }
    #[doc = "`AdditionalFileSystemPermissions`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"entries\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/FileSystemSandboxEntry\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"globScanMaxDepth\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"uint\","]
    #[doc = "          \"minimum\": 1.0"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"read\": {"]
    #[doc = "      \"description\": \"This will be removed in favor of `entries`.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"This will be removed in favor of `entries`.\","]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/LegacyAppPathString\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"write\": {"]
    #[doc = "      \"description\": \"This will be removed in favor of `entries`.\","]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"description\": \"This will be removed in favor of `entries`.\","]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/LegacyAppPathString\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct AdditionalFileSystemPermissions {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub entries: ::std::option::Option<::std::vec::Vec<FileSystemSandboxEntry>>,
        #[serde(
            rename = "globScanMaxDepth",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub glob_scan_max_depth: ::std::option::Option<::std::num::NonZeroU32>,
        #[doc = "This will be removed in favor of `entries`."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub read: ::std::option::Option<::std::vec::Vec<LegacyAppPathString>>,
        #[doc = "This will be removed in favor of `entries`."]
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub write: ::std::option::Option<::std::vec::Vec<LegacyAppPathString>>,
    }
    impl ::std::default::Default for AdditionalFileSystemPermissions {
        fn default() -> Self {
            Self {
                entries: Default::default(),
                glob_scan_max_depth: Default::default(),
                read: Default::default(),
                write: Default::default(),
            }
        }
    }
    #[doc = "`AdditionalNetworkPermissions`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"enabled\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"boolean\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct AdditionalNetworkPermissions {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub enabled: ::std::option::Option<bool>,
    }
    impl ::std::default::Default for AdditionalNetworkPermissions {
        fn default() -> Self {
            Self {
                enabled: Default::default(),
            }
        }
    }
    #[doc = "`FileSystemAccessMode`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"read\","]
    #[doc = "    \"write\","]
    #[doc = "    \"deny\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum FileSystemAccessMode {
        #[serde(rename = "read")]
        Read,
        #[serde(rename = "write")]
        Write,
        #[serde(rename = "deny")]
        Deny,
    }
    impl ::std::fmt::Display for FileSystemAccessMode {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Read => f.write_str("read"),
                Self::Write => f.write_str("write"),
                Self::Deny => f.write_str("deny"),
            }
        }
    }
    impl ::std::str::FromStr for FileSystemAccessMode {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "read" => Ok(Self::Read),
                "write" => Ok(Self::Write),
                "deny" => Ok(Self::Deny),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for FileSystemAccessMode {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for FileSystemAccessMode {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for FileSystemAccessMode {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`FileSystemPath`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"PathFileSystemPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"path\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"$ref\": \"#/definitions/LegacyAppPathString\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"PathFileSystemPathType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"path\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"GlobPatternFileSystemPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"pattern\","]
    #[doc = "        \"type\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"pattern\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"GlobPatternFileSystemPathType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"glob_pattern\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SpecialFileSystemPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"type\","]
    #[doc = "        \"value\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"type\": {"]
    #[doc = "          \"title\": \"SpecialFileSystemPathType\","]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"special\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"value\": {"]
    #[doc = "          \"$ref\": \"#/definitions/FileSystemSpecialPath\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "type")]
    pub enum FileSystemPath {
        #[doc = "PathFileSystemPath"]
        #[serde(rename = "path")]
        Path { path: LegacyAppPathString },
        #[doc = "GlobPatternFileSystemPath"]
        #[serde(rename = "glob_pattern")]
        GlobPattern { pattern: ::std::string::String },
        #[doc = "SpecialFileSystemPath"]
        #[serde(rename = "special")]
        Special { value: FileSystemSpecialPath },
    }
    #[doc = "`FileSystemSandboxEntry`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"access\","]
    #[doc = "    \"path\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"access\": {"]
    #[doc = "      \"$ref\": \"#/definitions/FileSystemAccessMode\""]
    #[doc = "    },"]
    #[doc = "    \"path\": {"]
    #[doc = "      \"$ref\": \"#/definitions/FileSystemPath\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct FileSystemSandboxEntry {
        pub access: FileSystemAccessMode,
        pub path: FileSystemPath,
    }
    #[doc = "`FileSystemSpecialPath`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"title\": \"RootFileSystemSpecialPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"kind\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"root\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"MinimalFileSystemSpecialPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"kind\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"minimal\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"KindFileSystemSpecialPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"kind\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"project_roots\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"subpath\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"TmpdirFileSystemSpecialPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"kind\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"tmpdir\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"title\": \"SlashTmpFileSystemSpecialPath\","]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"kind\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"slash_tmp\""]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"kind\","]
    #[doc = "        \"path\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"kind\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"unknown\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"path\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"subpath\": {"]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "kind")]
    pub enum FileSystemSpecialPath {
        #[serde(rename = "root")]
        Root,
        #[serde(rename = "minimal")]
        Minimal,
        #[doc = "KindFileSystemSpecialPath"]
        #[serde(rename = "project_roots")]
        ProjectRoots {
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            subpath: ::std::option::Option<::std::string::String>,
        },
        #[serde(rename = "tmpdir")]
        Tmpdir,
        #[serde(rename = "slash_tmp")]
        SlashTmp,
        #[serde(rename = "unknown")]
        Unknown {
            path: ::std::string::String,
            #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
            subpath: ::std::option::Option<::std::string::String>,
        },
    }
    #[doc = "`LegacyAppPathString`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\""]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    #[serde(transparent)]
    pub struct LegacyAppPathString(pub ::std::string::String);
    impl ::std::ops::Deref for LegacyAppPathString {
        type Target = ::std::string::String;
        fn deref(&self) -> &::std::string::String {
            &self.0
        }
    }
    impl ::std::convert::From<LegacyAppPathString> for ::std::string::String {
        fn from(value: LegacyAppPathString) -> Self {
            value.0
        }
    }
    impl ::std::convert::From<::std::string::String> for LegacyAppPathString {
        fn from(value: ::std::string::String) -> Self {
            Self(value)
        }
    }
    impl ::std::str::FromStr for LegacyAppPathString {
        type Err = ::std::convert::Infallible;
        fn from_str(value: &str) -> ::std::result::Result<Self, Self::Err> {
            Ok(Self(value.to_string()))
        }
    }
    impl ::std::fmt::Display for LegacyAppPathString {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            self.0.fmt(f)
        }
    }
    #[doc = "`PermissionsRequestApprovalParams`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"PermissionsRequestApprovalParams\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"cwd\","]
    #[doc = "    \"itemId\","]
    #[doc = "    \"permissions\","]
    #[doc = "    \"startedAtMs\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"cwd\": {"]
    #[doc = "      \"$ref\": \"#/definitions/AbsolutePathBuf\""]
    #[doc = "    },"]
    #[doc = "    \"environmentId\": {"]
    #[doc = "      \"default\": null,"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"default\": null,"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"itemId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"permissions\": {"]
    #[doc = "      \"$ref\": \"#/definitions/RequestPermissionProfile\""]
    #[doc = "    },"]
    #[doc = "    \"reason\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"startedAtMs\": {"]
    #[doc = "      \"description\": \"Unix timestamp (in milliseconds) when this approval request started.\","]
    #[doc = "      \"type\": \"integer\","]
    #[doc = "      \"format\": \"int64\""]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct PermissionsRequestApprovalParams {
        pub cwd: AbsolutePathBuf,
        #[serde(rename = "environmentId", default)]
        pub environment_id: ::std::option::Option<::std::string::String>,
        #[serde(rename = "itemId")]
        pub item_id: ::std::string::String,
        pub permissions: RequestPermissionProfile,
        #[serde(default)]
        pub reason: ::std::option::Option<::std::string::String>,
        #[doc = "Unix timestamp (in milliseconds) when this approval request started."]
        #[serde(rename = "startedAtMs")]
        #[ts(type = "number")]
        pub started_at_ms: i64,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "`RequestPermissionProfile`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"fileSystem\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/AdditionalFileSystemPermissions\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"network\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/AdditionalNetworkPermissions\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"additionalProperties\": false"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(deny_unknown_fields)]
    pub struct RequestPermissionProfile {
        #[serde(rename = "fileSystem", default)]
        pub file_system: ::std::option::Option<AdditionalFileSystemPermissions>,
        #[serde(default)]
        pub network: ::std::option::Option<AdditionalNetworkPermissions>,
    }
    impl ::std::default::Default for RequestPermissionProfile {
        fn default() -> Self {
            Self {
                file_system: Default::default(),
                network: Default::default(),
            }
        }
    }
}
pub mod tool_request_user_input_params {
    #[doc = r" Error types."]
    pub mod error {
        #[doc = r" Error from a `TryFrom` or `FromStr` implementation."]
        pub struct ConversionError(::std::borrow::Cow<'static, str>);
        impl ::std::error::Error for ConversionError {}
        impl ::std::fmt::Display for ConversionError {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                ::std::fmt::Display::fmt(&self.0, f)
            }
        }
        impl ::std::fmt::Debug for ConversionError {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                ::std::fmt::Debug::fmt(&self.0, f)
            }
        }
        impl From<&'static str> for ConversionError {
            fn from(value: &'static str) -> Self {
                Self(value.into())
            }
        }
        impl From<String> for ConversionError {
            fn from(value: String) -> Self {
                Self(value.into())
            }
        }
    }
    #[doc = "EXPERIMENTAL. Defines a single selectable option for request_user_input."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"EXPERIMENTAL. Defines a single selectable option for request_user_input.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"description\","]
    #[doc = "    \"label\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"description\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"label\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ToolRequestUserInputOption {
        pub description: ::std::string::String,
        pub label: ::std::string::String,
    }
    #[doc = "EXPERIMENTAL. Params sent with a request_user_input event."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"ToolRequestUserInputParams\","]
    #[doc = "  \"description\": \"EXPERIMENTAL. Params sent with a request_user_input event.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"itemId\","]
    #[doc = "    \"questions\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"autoResolutionMs\": {"]
    #[doc = "      \"default\": null,"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"default\": null,"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"uint64\","]
    #[doc = "          \"minimum\": 0.0"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"itemId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"questions\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"$ref\": \"#/definitions/ToolRequestUserInputQuestion\""]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ToolRequestUserInputParams {
        #[serde(rename = "autoResolutionMs", default)]
        #[ts(type = "number | null")]
        pub auto_resolution_ms: ::std::option::Option<u64>,
        #[serde(rename = "itemId")]
        pub item_id: ::std::string::String,
        pub questions: ::std::vec::Vec<ToolRequestUserInputQuestion>,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
    #[doc = "EXPERIMENTAL. Represents one request_user_input question and its required options."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"EXPERIMENTAL. Represents one request_user_input question and its required options.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"header\","]
    #[doc = "    \"id\","]
    #[doc = "    \"question\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"header\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"id\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"isOther\": {"]
    #[doc = "      \"default\": false,"]
    #[doc = "      \"type\": \"boolean\""]
    #[doc = "    },"]
    #[doc = "    \"isSecret\": {"]
    #[doc = "      \"default\": false,"]
    #[doc = "      \"type\": \"boolean\""]
    #[doc = "    },"]
    #[doc = "    \"options\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"$ref\": \"#/definitions/ToolRequestUserInputOption\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"question\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct ToolRequestUserInputQuestion {
        pub header: ::std::string::String,
        pub id: ::std::string::String,
        #[serde(rename = "isOther", default)]
        pub is_other: bool,
        #[serde(rename = "isSecret", default)]
        pub is_secret: bool,
        #[serde(default)]
        pub options: ::std::option::Option<::std::vec::Vec<ToolRequestUserInputOption>>,
        pub question: ::std::string::String,
    }
}
pub mod dynamic_tool_call_params {
    #[doc = r" Error types."]
    pub mod error {
        #[doc = r" Error from a `TryFrom` or `FromStr` implementation."]
        pub struct ConversionError(::std::borrow::Cow<'static, str>);
        impl ::std::error::Error for ConversionError {}
        impl ::std::fmt::Display for ConversionError {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                ::std::fmt::Display::fmt(&self.0, f)
            }
        }
        impl ::std::fmt::Debug for ConversionError {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                ::std::fmt::Debug::fmt(&self.0, f)
            }
        }
        impl From<&'static str> for ConversionError {
            fn from(value: &'static str) -> Self {
                Self(value.into())
            }
        }
        impl From<String> for ConversionError {
            fn from(value: String) -> Self {
                Self(value.into())
            }
        }
    }
    #[doc = "`DynamicToolCallParams`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"DynamicToolCallParams\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"arguments\","]
    #[doc = "    \"callId\","]
    #[doc = "    \"threadId\","]
    #[doc = "    \"tool\","]
    #[doc = "    \"turnId\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"arguments\": true,"]
    #[doc = "    \"callId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"namespace\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"threadId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"tool\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"turnId\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  }"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    pub struct DynamicToolCallParams {
        pub arguments: ::serde_json::Value,
        #[serde(rename = "callId")]
        pub call_id: ::std::string::String,
        #[serde(default)]
        pub namespace: ::std::option::Option<::std::string::String>,
        #[serde(rename = "threadId")]
        pub thread_id: ::std::string::String,
        pub tool: ::std::string::String,
        #[serde(rename = "turnId")]
        pub turn_id: ::std::string::String,
    }
}
pub mod mcp_server_elicitation_request_params {
    #[doc = r" Error types."]
    pub mod error {
        #[doc = r" Error from a `TryFrom` or `FromStr` implementation."]
        pub struct ConversionError(::std::borrow::Cow<'static, str>);
        impl ::std::error::Error for ConversionError {}
        impl ::std::fmt::Display for ConversionError {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                ::std::fmt::Display::fmt(&self.0, f)
            }
        }
        impl ::std::fmt::Debug for ConversionError {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> Result<(), ::std::fmt::Error> {
                ::std::fmt::Debug::fmt(&self.0, f)
            }
        }
        impl From<&'static str> for ConversionError {
            fn from(value: &'static str) -> Self {
                Self(value.into())
            }
        }
        impl From<String> for ConversionError {
            fn from(value: String) -> Self {
                Self(value.into())
            }
        }
    }
    #[doc = "`McpElicitationArrayType`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"array\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum McpElicitationArrayType {
        #[serde(rename = "array")]
        Array,
    }
    impl ::std::fmt::Display for McpElicitationArrayType {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Array => f.write_str("array"),
            }
        }
    }
    impl ::std::str::FromStr for McpElicitationArrayType {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "array" => Ok(Self::Array),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for McpElicitationArrayType {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for McpElicitationArrayType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for McpElicitationArrayType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`McpElicitationBooleanSchema`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"type\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"default\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"boolean\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"description\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"title\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"type\": {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationBooleanType\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"additionalProperties\": false"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(deny_unknown_fields)]
    pub struct McpElicitationBooleanSchema {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub default: ::std::option::Option<bool>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub description: ::std::option::Option<::std::string::String>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub title: ::std::option::Option<::std::string::String>,
        #[serde(rename = "type")]
        pub type_: McpElicitationBooleanType,
    }
    #[doc = "`McpElicitationBooleanType`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"boolean\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum McpElicitationBooleanType {
        #[serde(rename = "boolean")]
        Boolean,
    }
    impl ::std::fmt::Display for McpElicitationBooleanType {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Boolean => f.write_str("boolean"),
            }
        }
    }
    impl ::std::str::FromStr for McpElicitationBooleanType {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "boolean" => Ok(Self::Boolean),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for McpElicitationBooleanType {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for McpElicitationBooleanType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for McpElicitationBooleanType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`McpElicitationConstOption`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"const\","]
    #[doc = "    \"title\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"const\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    },"]
    #[doc = "    \"title\": {"]
    #[doc = "      \"type\": \"string\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"additionalProperties\": false"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(deny_unknown_fields)]
    pub struct McpElicitationConstOption {
        #[serde(rename = "const")]
        pub const_: ::std::string::String,
        pub title: ::std::string::String,
    }
    #[doc = "`McpElicitationEnumSchema`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationSingleSelectEnumSchema\""]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationMultiSelectEnumSchema\""]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationLegacyTitledEnumSchema\""]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(untagged)]
    pub enum McpElicitationEnumSchema {
        SingleSelectEnumSchema(McpElicitationSingleSelectEnumSchema),
        MultiSelectEnumSchema(McpElicitationMultiSelectEnumSchema),
        LegacyTitledEnumSchema(McpElicitationLegacyTitledEnumSchema),
    }
    impl ::std::convert::From<McpElicitationSingleSelectEnumSchema> for McpElicitationEnumSchema {
        fn from(value: McpElicitationSingleSelectEnumSchema) -> Self {
            Self::SingleSelectEnumSchema(value)
        }
    }
    impl ::std::convert::From<McpElicitationMultiSelectEnumSchema> for McpElicitationEnumSchema {
        fn from(value: McpElicitationMultiSelectEnumSchema) -> Self {
            Self::MultiSelectEnumSchema(value)
        }
    }
    impl ::std::convert::From<McpElicitationLegacyTitledEnumSchema> for McpElicitationEnumSchema {
        fn from(value: McpElicitationLegacyTitledEnumSchema) -> Self {
            Self::LegacyTitledEnumSchema(value)
        }
    }
    #[doc = "`McpElicitationLegacyTitledEnumSchema`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"enum\","]
    #[doc = "    \"type\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"default\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"description\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"enum\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"type\": \"string\""]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    \"enumNames\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"type\": \"string\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"title\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"type\": {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationStringType\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"additionalProperties\": false"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(deny_unknown_fields)]
    pub struct McpElicitationLegacyTitledEnumSchema {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub default: ::std::option::Option<::std::string::String>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub description: ::std::option::Option<::std::string::String>,
        #[serde(rename = "enum")]
        pub enum_: ::std::vec::Vec<::std::string::String>,
        #[serde(
            rename = "enumNames",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub enum_names: ::std::option::Option<::std::vec::Vec<::std::string::String>>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub title: ::std::option::Option<::std::string::String>,
        #[serde(rename = "type")]
        pub type_: McpElicitationStringType,
    }
    #[doc = "`McpElicitationMultiSelectEnumSchema`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationUntitledMultiSelectEnumSchema\""]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationTitledMultiSelectEnumSchema\""]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(untagged)]
    pub enum McpElicitationMultiSelectEnumSchema {
        UntitledMultiSelectEnumSchema(McpElicitationUntitledMultiSelectEnumSchema),
        TitledMultiSelectEnumSchema(McpElicitationTitledMultiSelectEnumSchema),
    }
    impl ::std::convert::From<McpElicitationUntitledMultiSelectEnumSchema>
        for McpElicitationMultiSelectEnumSchema
    {
        fn from(value: McpElicitationUntitledMultiSelectEnumSchema) -> Self {
            Self::UntitledMultiSelectEnumSchema(value)
        }
    }
    impl ::std::convert::From<McpElicitationTitledMultiSelectEnumSchema>
        for McpElicitationMultiSelectEnumSchema
    {
        fn from(value: McpElicitationTitledMultiSelectEnumSchema) -> Self {
            Self::TitledMultiSelectEnumSchema(value)
        }
    }
    #[doc = "`McpElicitationNumberSchema`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"type\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"default\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"number\","]
    #[doc = "          \"format\": \"double\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"description\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"maximum\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"number\","]
    #[doc = "          \"format\": \"double\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"minimum\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"number\","]
    #[doc = "          \"format\": \"double\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"title\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"type\": {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationNumberType\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"additionalProperties\": false"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(deny_unknown_fields)]
    pub struct McpElicitationNumberSchema {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub default: ::std::option::Option<f64>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub description: ::std::option::Option<::std::string::String>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub maximum: ::std::option::Option<f64>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub minimum: ::std::option::Option<f64>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub title: ::std::option::Option<::std::string::String>,
        #[serde(rename = "type")]
        pub type_: McpElicitationNumberType,
    }
    #[doc = "`McpElicitationNumberType`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"number\","]
    #[doc = "    \"integer\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum McpElicitationNumberType {
        #[serde(rename = "number")]
        Number,
        #[serde(rename = "integer")]
        Integer,
    }
    impl ::std::fmt::Display for McpElicitationNumberType {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Number => f.write_str("number"),
                Self::Integer => f.write_str("integer"),
            }
        }
    }
    impl ::std::str::FromStr for McpElicitationNumberType {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "number" => Ok(Self::Number),
                "integer" => Ok(Self::Integer),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for McpElicitationNumberType {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for McpElicitationNumberType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for McpElicitationNumberType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`McpElicitationObjectType`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"object\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum McpElicitationObjectType {
        #[serde(rename = "object")]
        Object,
    }
    impl ::std::fmt::Display for McpElicitationObjectType {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Object => f.write_str("object"),
            }
        }
    }
    impl ::std::str::FromStr for McpElicitationObjectType {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "object" => Ok(Self::Object),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for McpElicitationObjectType {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for McpElicitationObjectType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for McpElicitationObjectType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`McpElicitationPrimitiveSchema`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationEnumSchema\""]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationStringSchema\""]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationNumberSchema\""]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationBooleanSchema\""]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(untagged)]
    pub enum McpElicitationPrimitiveSchema {
        EnumSchema(McpElicitationEnumSchema),
        StringSchema(McpElicitationStringSchema),
        NumberSchema(McpElicitationNumberSchema),
        BooleanSchema(McpElicitationBooleanSchema),
    }
    impl ::std::convert::From<McpElicitationEnumSchema> for McpElicitationPrimitiveSchema {
        fn from(value: McpElicitationEnumSchema) -> Self {
            Self::EnumSchema(value)
        }
    }
    impl ::std::convert::From<McpElicitationStringSchema> for McpElicitationPrimitiveSchema {
        fn from(value: McpElicitationStringSchema) -> Self {
            Self::StringSchema(value)
        }
    }
    impl ::std::convert::From<McpElicitationNumberSchema> for McpElicitationPrimitiveSchema {
        fn from(value: McpElicitationNumberSchema) -> Self {
            Self::NumberSchema(value)
        }
    }
    impl ::std::convert::From<McpElicitationBooleanSchema> for McpElicitationPrimitiveSchema {
        fn from(value: McpElicitationBooleanSchema) -> Self {
            Self::BooleanSchema(value)
        }
    }
    #[doc = "Typed form schema for MCP `elicitation/create` requests.\n\nThis matches the `requestedSchema` shape from the MCP 2025-11-25 `ElicitRequestFormParams` schema."]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"description\": \"Typed form schema for MCP `elicitation/create` requests.\\n\\nThis matches the `requestedSchema` shape from the MCP 2025-11-25 `ElicitRequestFormParams` schema.\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"properties\","]
    #[doc = "    \"type\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"$schema\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"properties\": {"]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"additionalProperties\": {"]
    #[doc = "        \"$ref\": \"#/definitions/McpElicitationPrimitiveSchema\""]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    \"required\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"type\": \"string\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"type\": {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationObjectType\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"additionalProperties\": false"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(deny_unknown_fields)]
    pub struct McpElicitationSchema {
        pub properties:
            ::std::collections::HashMap<::std::string::String, McpElicitationPrimitiveSchema>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub required: ::std::option::Option<::std::vec::Vec<::std::string::String>>,
        #[serde(
            rename = "$schema",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub schema: ::std::option::Option<::std::string::String>,
        #[serde(rename = "type")]
        pub type_: McpElicitationObjectType,
    }
    #[doc = "`McpElicitationSingleSelectEnumSchema`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationUntitledSingleSelectEnumSchema\""]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationTitledSingleSelectEnumSchema\""]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(untagged)]
    pub enum McpElicitationSingleSelectEnumSchema {
        UntitledSingleSelectEnumSchema(McpElicitationUntitledSingleSelectEnumSchema),
        TitledSingleSelectEnumSchema(McpElicitationTitledSingleSelectEnumSchema),
    }
    impl ::std::convert::From<McpElicitationUntitledSingleSelectEnumSchema>
        for McpElicitationSingleSelectEnumSchema
    {
        fn from(value: McpElicitationUntitledSingleSelectEnumSchema) -> Self {
            Self::UntitledSingleSelectEnumSchema(value)
        }
    }
    impl ::std::convert::From<McpElicitationTitledSingleSelectEnumSchema>
        for McpElicitationSingleSelectEnumSchema
    {
        fn from(value: McpElicitationTitledSingleSelectEnumSchema) -> Self {
            Self::TitledSingleSelectEnumSchema(value)
        }
    }
    #[doc = "`McpElicitationStringFormat`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"email\","]
    #[doc = "    \"uri\","]
    #[doc = "    \"date\","]
    #[doc = "    \"date-time\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum McpElicitationStringFormat {
        #[serde(rename = "email")]
        Email,
        #[serde(rename = "uri")]
        Uri,
        #[serde(rename = "date")]
        Date,
        #[serde(rename = "date-time")]
        DateTime,
    }
    impl ::std::fmt::Display for McpElicitationStringFormat {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::Email => f.write_str("email"),
                Self::Uri => f.write_str("uri"),
                Self::Date => f.write_str("date"),
                Self::DateTime => f.write_str("date-time"),
            }
        }
    }
    impl ::std::str::FromStr for McpElicitationStringFormat {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "email" => Ok(Self::Email),
                "uri" => Ok(Self::Uri),
                "date" => Ok(Self::Date),
                "date-time" => Ok(Self::DateTime),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for McpElicitationStringFormat {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for McpElicitationStringFormat {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for McpElicitationStringFormat {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`McpElicitationStringSchema`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"type\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"default\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"description\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"format\": {"]
    #[doc = "      \"anyOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"$ref\": \"#/definitions/McpElicitationStringFormat\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"maxLength\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"uint32\","]
    #[doc = "          \"minimum\": 0.0"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"minLength\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"uint32\","]
    #[doc = "          \"minimum\": 0.0"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"title\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"type\": {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationStringType\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"additionalProperties\": false"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(deny_unknown_fields)]
    pub struct McpElicitationStringSchema {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub default: ::std::option::Option<::std::string::String>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub description: ::std::option::Option<::std::string::String>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub format: ::std::option::Option<McpElicitationStringFormat>,
        #[serde(
            rename = "maxLength",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub max_length: ::std::option::Option<u32>,
        #[serde(
            rename = "minLength",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        pub min_length: ::std::option::Option<u32>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub title: ::std::option::Option<::std::string::String>,
        #[serde(rename = "type")]
        pub type_: McpElicitationStringType,
    }
    #[doc = "`McpElicitationStringType`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"string\","]
    #[doc = "  \"enum\": ["]
    #[doc = "    \"string\""]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Copy,
        Debug,
        Eq,
        Hash,
        Ord,
        PartialEq,
        PartialOrd,
    )]
    pub enum McpElicitationStringType {
        #[serde(rename = "string")]
        String,
    }
    impl ::std::fmt::Display for McpElicitationStringType {
        fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
            match *self {
                Self::String => f.write_str("string"),
            }
        }
    }
    impl ::std::str::FromStr for McpElicitationStringType {
        type Err = self::error::ConversionError;
        fn from_str(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            match value {
                "string" => Ok(Self::String),
                _ => Err("invalid value".into()),
            }
        }
    }
    impl ::std::convert::TryFrom<&str> for McpElicitationStringType {
        type Error = self::error::ConversionError;
        fn try_from(value: &str) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<&::std::string::String> for McpElicitationStringType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: &::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    impl ::std::convert::TryFrom<::std::string::String> for McpElicitationStringType {
        type Error = self::error::ConversionError;
        fn try_from(
            value: ::std::string::String,
        ) -> ::std::result::Result<Self, self::error::ConversionError> {
            value.parse()
        }
    }
    #[doc = "`McpElicitationTitledEnumItems`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"anyOf\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"anyOf\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"$ref\": \"#/definitions/McpElicitationConstOption\""]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"additionalProperties\": false"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(deny_unknown_fields)]
    pub struct McpElicitationTitledEnumItems {
        #[serde(rename = "anyOf")]
        pub any_of: ::std::vec::Vec<McpElicitationConstOption>,
    }
    #[doc = "`McpElicitationTitledMultiSelectEnumSchema`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"items\","]
    #[doc = "    \"type\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"default\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"type\": \"string\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"description\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"items\": {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationTitledEnumItems\""]
    #[doc = "    },"]
    #[doc = "    \"maxItems\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"uint64\","]
    #[doc = "          \"minimum\": 0.0"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"minItems\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"uint64\","]
    #[doc = "          \"minimum\": 0.0"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"title\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"type\": {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationArrayType\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"additionalProperties\": false"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(deny_unknown_fields)]
    pub struct McpElicitationTitledMultiSelectEnumSchema {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub default: ::std::option::Option<::std::vec::Vec<::std::string::String>>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub description: ::std::option::Option<::std::string::String>,
        pub items: McpElicitationTitledEnumItems,
        #[serde(
            rename = "maxItems",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[ts(type = "number | null")]
        pub max_items: ::std::option::Option<u64>,
        #[serde(
            rename = "minItems",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[ts(type = "number | null")]
        pub min_items: ::std::option::Option<u64>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub title: ::std::option::Option<::std::string::String>,
        #[serde(rename = "type")]
        pub type_: McpElicitationArrayType,
    }
    #[doc = "`McpElicitationTitledSingleSelectEnumSchema`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"oneOf\","]
    #[doc = "    \"type\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"default\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"description\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"oneOf\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"$ref\": \"#/definitions/McpElicitationConstOption\""]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    \"title\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"type\": {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationStringType\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"additionalProperties\": false"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(deny_unknown_fields)]
    pub struct McpElicitationTitledSingleSelectEnumSchema {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub default: ::std::option::Option<::std::string::String>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub description: ::std::option::Option<::std::string::String>,
        #[serde(rename = "oneOf")]
        pub one_of: ::std::vec::Vec<McpElicitationConstOption>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub title: ::std::option::Option<::std::string::String>,
        #[serde(rename = "type")]
        pub type_: McpElicitationStringType,
    }
    #[doc = "`McpElicitationUntitledEnumItems`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"enum\","]
    #[doc = "    \"type\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"enum\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"type\": \"string\""]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    \"type\": {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationStringType\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"additionalProperties\": false"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(deny_unknown_fields)]
    pub struct McpElicitationUntitledEnumItems {
        #[serde(rename = "enum")]
        pub enum_: ::std::vec::Vec<::std::string::String>,
        #[serde(rename = "type")]
        pub type_: McpElicitationStringType,
    }
    #[doc = "`McpElicitationUntitledMultiSelectEnumSchema`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"items\","]
    #[doc = "    \"type\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"default\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"array\","]
    #[doc = "          \"items\": {"]
    #[doc = "            \"type\": \"string\""]
    #[doc = "          }"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"description\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"items\": {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationUntitledEnumItems\""]
    #[doc = "    },"]
    #[doc = "    \"maxItems\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"uint64\","]
    #[doc = "          \"minimum\": 0.0"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"minItems\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"integer\","]
    #[doc = "          \"format\": \"uint64\","]
    #[doc = "          \"minimum\": 0.0"]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"title\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"type\": {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationArrayType\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"additionalProperties\": false"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(deny_unknown_fields)]
    pub struct McpElicitationUntitledMultiSelectEnumSchema {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub default: ::std::option::Option<::std::vec::Vec<::std::string::String>>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub description: ::std::option::Option<::std::string::String>,
        pub items: McpElicitationUntitledEnumItems,
        #[serde(
            rename = "maxItems",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[ts(type = "number | null")]
        pub max_items: ::std::option::Option<u64>,
        #[serde(
            rename = "minItems",
            default,
            skip_serializing_if = "::std::option::Option::is_none"
        )]
        #[ts(type = "number | null")]
        pub min_items: ::std::option::Option<u64>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub title: ::std::option::Option<::std::string::String>,
        #[serde(rename = "type")]
        pub type_: McpElicitationArrayType,
    }
    #[doc = "`McpElicitationUntitledSingleSelectEnumSchema`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"required\": ["]
    #[doc = "    \"enum\","]
    #[doc = "    \"type\""]
    #[doc = "  ],"]
    #[doc = "  \"properties\": {"]
    #[doc = "    \"default\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"description\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"enum\": {"]
    #[doc = "      \"type\": \"array\","]
    #[doc = "      \"items\": {"]
    #[doc = "        \"type\": \"string\""]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    \"title\": {"]
    #[doc = "      \"oneOf\": ["]
    #[doc = "        {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        {"]
    #[doc = "          \"type\": \"null\""]
    #[doc = "        }"]
    #[doc = "      ]"]
    #[doc = "    },"]
    #[doc = "    \"type\": {"]
    #[doc = "      \"$ref\": \"#/definitions/McpElicitationStringType\""]
    #[doc = "    }"]
    #[doc = "  },"]
    #[doc = "  \"additionalProperties\": false"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(deny_unknown_fields)]
    pub struct McpElicitationUntitledSingleSelectEnumSchema {
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub default: ::std::option::Option<::std::string::String>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub description: ::std::option::Option<::std::string::String>,
        #[serde(rename = "enum")]
        pub enum_: ::std::vec::Vec<::std::string::String>,
        #[serde(default, skip_serializing_if = "::std::option::Option::is_none")]
        pub title: ::std::option::Option<::std::string::String>,
        #[serde(rename = "type")]
        pub type_: McpElicitationStringType,
    }
    #[doc = "`McpServerElicitationRequestParams`"]
    #[doc = r""]
    #[doc = r" <details><summary>JSON schema</summary>"]
    #[doc = r""]
    #[doc = r" ```json"]
    #[doc = "{"]
    #[doc = "  \"title\": \"McpServerElicitationRequestParams\","]
    #[doc = "  \"type\": \"object\","]
    #[doc = "  \"oneOf\": ["]
    #[doc = "    {"]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"message\","]
    #[doc = "        \"mode\","]
    #[doc = "        \"requestedSchema\","]
    #[doc = "        \"serverName\","]
    #[doc = "        \"threadId\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"_meta\": true,"]
    #[doc = "        \"message\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"mode\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"form\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"requestedSchema\": {"]
    #[doc = "          \"$ref\": \"#/definitions/McpElicitationSchema\""]
    #[doc = "        },"]
    #[doc = "        \"serverName\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"threadId\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"turnId\": {"]
    #[doc = "          \"description\": \"Active Codex turn when this elicitation was observed, if app-server could correlate one.\\n\\nThis is nullable because MCP models elicitation as a standalone server-to-client request identified by the MCP server request id. It may be triggered during a turn, but turn context is app-server correlation rather than part of the protocol identity of the elicitation itself.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"Active Codex turn when this elicitation was observed, if app-server could correlate one.\\n\\nThis is nullable because MCP models elicitation as a standalone server-to-client request identified by the MCP server request id. It may be triggered during a turn, but turn context is app-server correlation rather than part of the protocol identity of the elicitation itself.\","]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"message\","]
    #[doc = "        \"mode\","]
    #[doc = "        \"requestedSchema\","]
    #[doc = "        \"serverName\","]
    #[doc = "        \"threadId\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"_meta\": true,"]
    #[doc = "        \"message\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"mode\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"openai/form\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"requestedSchema\": true,"]
    #[doc = "        \"serverName\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"threadId\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"turnId\": {"]
    #[doc = "          \"description\": \"Active Codex turn when this elicitation was observed, if app-server could correlate one.\\n\\nThis is nullable because MCP models elicitation as a standalone server-to-client request identified by the MCP server request id. It may be triggered during a turn, but turn context is app-server correlation rather than part of the protocol identity of the elicitation itself.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"Active Codex turn when this elicitation was observed, if app-server could correlate one.\\n\\nThis is nullable because MCP models elicitation as a standalone server-to-client request identified by the MCP server request id. It may be triggered during a turn, but turn context is app-server correlation rather than part of the protocol identity of the elicitation itself.\","]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    },"]
    #[doc = "    {"]
    #[doc = "      \"type\": \"object\","]
    #[doc = "      \"required\": ["]
    #[doc = "        \"elicitationId\","]
    #[doc = "        \"message\","]
    #[doc = "        \"mode\","]
    #[doc = "        \"serverName\","]
    #[doc = "        \"threadId\","]
    #[doc = "        \"url\""]
    #[doc = "      ],"]
    #[doc = "      \"properties\": {"]
    #[doc = "        \"_meta\": true,"]
    #[doc = "        \"elicitationId\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"message\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"mode\": {"]
    #[doc = "          \"type\": \"string\","]
    #[doc = "          \"enum\": ["]
    #[doc = "            \"url\""]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"serverName\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"threadId\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        },"]
    #[doc = "        \"turnId\": {"]
    #[doc = "          \"description\": \"Active Codex turn when this elicitation was observed, if app-server could correlate one.\\n\\nThis is nullable because MCP models elicitation as a standalone server-to-client request identified by the MCP server request id. It may be triggered during a turn, but turn context is app-server correlation rather than part of the protocol identity of the elicitation itself.\","]
    #[doc = "          \"oneOf\": ["]
    #[doc = "            {"]
    #[doc = "              \"description\": \"Active Codex turn when this elicitation was observed, if app-server could correlate one.\\n\\nThis is nullable because MCP models elicitation as a standalone server-to-client request identified by the MCP server request id. It may be triggered during a turn, but turn context is app-server correlation rather than part of the protocol identity of the elicitation itself.\","]
    #[doc = "              \"type\": \"string\""]
    #[doc = "            },"]
    #[doc = "            {"]
    #[doc = "              \"type\": \"null\""]
    #[doc = "            }"]
    #[doc = "          ]"]
    #[doc = "        },"]
    #[doc = "        \"url\": {"]
    #[doc = "          \"type\": \"string\""]
    #[doc = "        }"]
    #[doc = "      }"]
    #[doc = "    }"]
    #[doc = "  ]"]
    #[doc = "}"]
    #[doc = r" ```"]
    #[doc = r" </details>"]
    #[derive(
        :: schemars :: JsonSchema,
        :: serde :: Deserialize,
        :: serde :: Serialize,
        :: ts_rs :: TS,
        Clone,
        Debug,
        PartialEq,
    )]
    #[serde(tag = "mode")]
    pub enum McpServerElicitationRequestParams {
        #[serde(rename = "form")]
        Form {
            message: ::std::string::String,
            #[serde(rename = "_meta", default)]
            meta: ::std::option::Option<::serde_json::Value>,
            #[serde(rename = "requestedSchema")]
            requested_schema: McpElicitationSchema,
            #[serde(rename = "serverName")]
            server_name: ::std::string::String,
            #[serde(rename = "threadId")]
            thread_id: ::std::string::String,
            #[doc = "Active Codex turn when this elicitation was observed, if app-server could correlate one.\n\nThis is nullable because MCP models elicitation as a standalone server-to-client request identified by the MCP server request id. It may be triggered during a turn, but turn context is app-server correlation rather than part of the protocol identity of the elicitation itself."]
            #[serde(rename = "turnId", default)]
            turn_id: ::std::option::Option<::std::string::String>,
        },
        #[serde(rename = "openai/form")]
        OpenaiForm {
            message: ::std::string::String,
            #[serde(rename = "_meta", default)]
            meta: ::std::option::Option<::serde_json::Value>,
            #[serde(rename = "requestedSchema")]
            requested_schema: ::serde_json::Value,
            #[serde(rename = "serverName")]
            server_name: ::std::string::String,
            #[serde(rename = "threadId")]
            thread_id: ::std::string::String,
            #[doc = "Active Codex turn when this elicitation was observed, if app-server could correlate one.\n\nThis is nullable because MCP models elicitation as a standalone server-to-client request identified by the MCP server request id. It may be triggered during a turn, but turn context is app-server correlation rather than part of the protocol identity of the elicitation itself."]
            #[serde(rename = "turnId", default)]
            turn_id: ::std::option::Option<::std::string::String>,
        },
        #[serde(rename = "url")]
        Url {
            #[serde(rename = "elicitationId")]
            elicitation_id: ::std::string::String,
            message: ::std::string::String,
            #[serde(rename = "_meta", default)]
            meta: ::std::option::Option<::serde_json::Value>,
            #[serde(rename = "serverName")]
            server_name: ::std::string::String,
            #[serde(rename = "threadId")]
            thread_id: ::std::string::String,
            #[doc = "Active Codex turn when this elicitation was observed, if app-server could correlate one.\n\nThis is nullable because MCP models elicitation as a standalone server-to-client request identified by the MCP server request id. It may be triggered during a turn, but turn context is app-server correlation rather than part of the protocol identity of the elicitation itself."]
            #[serde(rename = "turnId", default)]
            turn_id: ::std::option::Option<::std::string::String>,
            url: ::std::string::String,
        },
    }
}

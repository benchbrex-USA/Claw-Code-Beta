use std::collections::{BTreeMap, BTreeSet};

use api::ToolDefinition;
use plugins::PluginTool;
use runtime::{
    permission_enforcer::PermissionEnforcer, McpDegradedReport, PermissionMode,
};
use serde_json::Value;

use crate::{
    deferred_tool_specs, execute_tool_with_enforcer, maybe_enforce_permission_check,
    mvp_tool_specs, normalize_tool_name, normalize_tool_search_query, permission_mode_from_plugin,
    search_tool_specs, SearchableToolSpec, ToolSearchOutput,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolManifestEntry {
    pub name: String,
    pub source: ToolSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSource {
    Base,
    Conditional,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolRegistry {
    entries: Vec<ToolManifestEntry>,
}

impl ToolRegistry {
    #[must_use]
    pub fn new(entries: Vec<ToolManifestEntry>) -> Self {
        Self { entries }
    }

    #[must_use]
    pub fn entries(&self) -> &[ToolManifestEntry] {
        &self.entries
    }
}

#[derive(Debug, Clone)]
pub struct GlobalToolRegistry {
    plugin_tools: Vec<PluginTool>,
    runtime_tools: Vec<RuntimeToolDefinition>,
    enforcer: Option<PermissionEnforcer>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeToolDefinition {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
    pub required_permission: PermissionMode,
}

#[derive(Debug, Clone, Copy)]
enum DispatchTarget<'a> {
    ToolSearch,
    Builtin,
    Runtime(&'a RuntimeToolDefinition),
    Plugin(&'a PluginTool),
}

impl GlobalToolRegistry {
    #[must_use]
    pub fn builtin() -> Self {
        Self {
            plugin_tools: Vec::new(),
            runtime_tools: Vec::new(),
            enforcer: None,
        }
    }

    pub fn with_plugin_tools(plugin_tools: Vec<PluginTool>) -> Result<Self, String> {
        let builtin_names = mvp_tool_specs()
            .into_iter()
            .map(|spec| spec.name.to_string())
            .collect::<BTreeSet<_>>();
        let mut seen_plugin_names = BTreeSet::new();

        for tool in &plugin_tools {
            let name = tool.definition().name.clone();
            if builtin_names.contains(&name) {
                return Err(format!(
                    "plugin tool `{name}` conflicts with a built-in tool name"
                ));
            }
            if !seen_plugin_names.insert(name.clone()) {
                return Err(format!("duplicate plugin tool name `{name}`"));
            }
        }

        Ok(Self {
            plugin_tools,
            runtime_tools: Vec::new(),
            enforcer: None,
        })
    }

    pub fn with_runtime_tools(
        mut self,
        runtime_tools: Vec<RuntimeToolDefinition>,
    ) -> Result<Self, String> {
        let mut seen_names = mvp_tool_specs()
            .into_iter()
            .map(|spec| spec.name.to_string())
            .chain(
                self.plugin_tools
                    .iter()
                    .map(|tool| tool.definition().name.clone()),
            )
            .collect::<BTreeSet<_>>();

        for tool in &runtime_tools {
            if !seen_names.insert(tool.name.clone()) {
                return Err(format!(
                    "runtime tool `{}` conflicts with an existing tool name",
                    tool.name
                ));
            }
        }

        self.runtime_tools = runtime_tools;
        Ok(self)
    }

    #[must_use]
    pub fn with_enforcer(mut self, enforcer: PermissionEnforcer) -> Self {
        self.set_enforcer(enforcer);
        self
    }

    pub fn normalize_allowed_tools(
        &self,
        values: &[String],
    ) -> Result<Option<BTreeSet<String>>, String> {
        if values.is_empty() {
            return Ok(None);
        }

        let builtin_specs = mvp_tool_specs();
        let canonical_names = builtin_specs
            .iter()
            .map(|spec| spec.name.to_string())
            .chain(
                self.plugin_tools
                    .iter()
                    .map(|tool| tool.definition().name.clone()),
            )
            .chain(self.runtime_tools.iter().map(|tool| tool.name.clone()))
            .collect::<Vec<_>>();
        let mut name_map = canonical_names
            .iter()
            .map(|name| (normalize_tool_name(name), name.clone()))
            .collect::<BTreeMap<_, _>>();

        for (alias, canonical) in [
            ("read", "read_file"),
            ("write", "write_file"),
            ("edit", "edit_file"),
            ("glob", "glob_search"),
            ("grep", "grep_search"),
        ] {
            name_map.insert(alias.to_string(), canonical.to_string());
        }

        let mut allowed = BTreeSet::new();
        for value in values {
            for token in value
                .split(|ch: char| ch == ',' || ch.is_whitespace())
                .filter(|token| !token.is_empty())
            {
                let normalized = normalize_tool_name(token);
                let canonical = name_map.get(&normalized).ok_or_else(|| {
                    format!(
                        "unsupported tool in --allowedTools: {token} (expected one of: {})",
                        canonical_names.join(", ")
                    )
                })?;
                allowed.insert(canonical.clone());
            }
        }

        Ok(Some(allowed))
    }

    #[must_use]
    pub fn definitions(&self, allowed_tools: Option<&BTreeSet<String>>) -> Vec<ToolDefinition> {
        let builtin = mvp_tool_specs()
            .into_iter()
            .filter(|spec| allowed_tools.is_none_or(|allowed| allowed.contains(spec.name)))
            .map(|spec| ToolDefinition {
                name: spec.name.to_string(),
                description: Some(spec.description.to_string()),
                input_schema: spec.input_schema,
            });
        let runtime = self
            .runtime_tools
            .iter()
            .filter(|tool| allowed_tools.is_none_or(|allowed| allowed.contains(tool.name.as_str())))
            .map(|tool| ToolDefinition {
                name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: tool.input_schema.clone(),
            });
        let plugin = self
            .plugin_tools
            .iter()
            .filter(|tool| {
                allowed_tools
                    .is_none_or(|allowed| allowed.contains(tool.definition().name.as_str()))
            })
            .map(|tool| ToolDefinition {
                name: tool.definition().name.clone(),
                description: tool.definition().description.clone(),
                input_schema: tool.definition().input_schema.clone(),
            });
        builtin.chain(runtime).chain(plugin).collect()
    }

    pub fn permission_specs(
        &self,
        allowed_tools: Option<&BTreeSet<String>>,
    ) -> Result<Vec<(String, PermissionMode)>, String> {
        let builtin = mvp_tool_specs()
            .into_iter()
            .filter(|spec| allowed_tools.is_none_or(|allowed| allowed.contains(spec.name)))
            .map(|spec| (spec.name.to_string(), spec.required_permission));
        let runtime = self
            .runtime_tools
            .iter()
            .filter(|tool| allowed_tools.is_none_or(|allowed| allowed.contains(tool.name.as_str())))
            .map(|tool| (tool.name.clone(), tool.required_permission));
        let plugin = self
            .plugin_tools
            .iter()
            .filter(|tool| {
                allowed_tools
                    .is_none_or(|allowed| allowed.contains(tool.definition().name.as_str()))
            })
            .map(|tool| {
                permission_mode_from_plugin(tool.required_permission())
                    .map(|permission| (tool.definition().name.clone(), permission))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(builtin.chain(runtime).chain(plugin).collect())
    }

    #[must_use]
    pub fn search(
        &self,
        query: &str,
        max_results: usize,
        pending_mcp_servers: Option<Vec<String>>,
        mcp_degraded: Option<McpDegradedReport>,
    ) -> ToolSearchOutput {
        let query = query.trim().to_string();
        let normalized_query = normalize_tool_search_query(&query);
        let matches = search_tool_specs(&query, max_results.max(1), &self.searchable_tool_specs());

        ToolSearchOutput {
            matches,
            query,
            normalized_query,
            total_deferred_tools: self.searchable_tool_specs().len(),
            pending_mcp_servers,
            mcp_degraded,
        }
    }

    pub fn set_enforcer(&mut self, enforcer: PermissionEnforcer) {
        self.enforcer = Some(enforcer);
    }

    pub fn enforce(&self, name: &str, input: &Value) -> Result<(), String> {
        maybe_enforce_permission_check(self.enforcer.as_ref(), name, input)
    }

    fn resolve_dispatch_target(&self, name: &str) -> Result<DispatchTarget<'_>, String> {
        if name == "ToolSearch" {
            return Ok(DispatchTarget::ToolSearch);
        }
        if mvp_tool_specs().iter().any(|spec| spec.name == name) {
            return Ok(DispatchTarget::Builtin);
        }
        if let Some(tool) = self.runtime_tools.iter().find(|tool| tool.name == name) {
            return Ok(DispatchTarget::Runtime(tool));
        }
        if let Some(tool) = self
            .plugin_tools
            .iter()
            .find(|tool| tool.definition().name == name)
        {
            return Ok(DispatchTarget::Plugin(tool));
        }
        Err(format!("unsupported tool: {name}"))
    }

    pub fn execute_with_handlers<SearchHandler, RuntimeHandler>(
        &self,
        name: &str,
        input: &Value,
        search_handler: SearchHandler,
        runtime_handler: RuntimeHandler,
    ) -> Result<String, String>
    where
        SearchHandler: FnOnce(&Value) -> Result<String, String>,
        RuntimeHandler: FnOnce(&RuntimeToolDefinition, &Value) -> Result<String, String>,
    {
        match self.resolve_dispatch_target(name)? {
            DispatchTarget::ToolSearch => {
                self.enforce(name, input)?;
                search_handler(input)
            }
            DispatchTarget::Builtin => {
                execute_tool_with_enforcer(self.enforcer.as_ref(), name, input)
            }
            DispatchTarget::Runtime(tool) => {
                self.enforce(name, input)?;
                runtime_handler(tool, input)
            }
            DispatchTarget::Plugin(tool) => {
                self.enforce(name, input)?;
                tool.execute(input).map_err(|error| error.to_string())
            }
        }
    }

    #[must_use]
    pub fn manifest(&self) -> ToolRegistry {
        let builtin = mvp_tool_specs().into_iter().map(|spec| ToolManifestEntry {
            name: spec.name.to_string(),
            source: ToolSource::Base,
        });
        let plugin = self.plugin_tools.iter().map(|tool| ToolManifestEntry {
            name: tool.definition().name.clone(),
            source: ToolSource::Conditional,
        });
        let runtime = self.runtime_tools.iter().map(|tool| ToolManifestEntry {
            name: tool.name.clone(),
            source: ToolSource::Conditional,
        });

        ToolRegistry::new(builtin.chain(plugin).chain(runtime).collect())
    }

    /// Execute a built-in tool, plugin tool, or `ToolSearch` directly.
    ///
    /// Compatibility note: `execute()` handles built-ins, plugin tools, and
    /// `ToolSearch`. Only runtime/MCP tools require
    /// [`GlobalToolRegistry::execute_with_handlers`].
    pub fn execute(&self, name: &str, input: &Value) -> Result<String, String> {
        match self.resolve_dispatch_target(name)? {
            DispatchTarget::ToolSearch | DispatchTarget::Builtin => {
                execute_tool_with_enforcer(self.enforcer.as_ref(), name, input)
            }
            DispatchTarget::Runtime(_) => Err(format!(
                "runtime tool `{name}` requires a runtime dispatcher; compatibility note: call GlobalToolRegistry::execute_with_handlers for runtime/MCP tools"
            )),
            DispatchTarget::Plugin(tool) => {
                self.enforce(name, input)?;
                tool.execute(input).map_err(|error| error.to_string())
            }
        }
    }

    fn searchable_tool_specs(&self) -> Vec<SearchableToolSpec> {
        let builtin = deferred_tool_specs()
            .into_iter()
            .map(|spec| SearchableToolSpec {
                name: spec.name.to_string(),
                description: spec.description.to_string(),
            });
        let runtime = self.runtime_tools.iter().map(|tool| SearchableToolSpec {
            name: tool.name.clone(),
            description: tool.description.clone().unwrap_or_default(),
        });
        let plugin = self.plugin_tools.iter().map(|tool| SearchableToolSpec {
            name: tool.definition().name.clone(),
            description: tool.definition().description.clone().unwrap_or_default(),
        });
        builtin.chain(runtime).chain(plugin).collect()
    }
}

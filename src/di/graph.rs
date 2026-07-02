use crate::registry::{self, Registry, CapabilityDependency as Dependency};

/// Builds and validates dependency graphs for capabilities.
pub struct DependencyGraph {
    root_id: String,
    _cap_def: registry::CapabilityDefinition,
}

impl DependencyGraph {
    pub fn new(root_id: String, cap_def: registry::CapabilityDefinition) -> Self {
       Self {
            root_id,
            _cap_def: cap_def,
        }
    }

    /// Detect cycles in the dependency graph using DFS.
    /// Returns a list of cycle paths if any are found.
    pub fn detect_cycles(&self) -> Result<Vec<Vec<String>>, anyhow::Error> {
        let mut cycles = Vec::new();
        let mut path = Vec::new();
        let mut visited = std::collections::HashSet::new();

        self.dfs_cycles(&self.root_id, &mut path, &mut visited, &mut cycles);

        Ok(cycles)
    }

    fn dfs_cycles(
        &self,
        current_id: &str,
        path: &mut Vec<String>,
        visited: &mut std::collections::HashSet<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        if path.contains(&current_id.to_string()) {
            let cycle_start = path.iter().position(|x| x == current_id).unwrap();
            cycles.push(path[cycle_start..].to_vec());
            return;
        }

        if visited.contains(current_id) {
            return;
        }

        path.push(current_id.to_string());
        visited.insert(current_id.to_string());

        let cap_def = registry::CAPABILITY_REGISTRY.get(current_id);
        if let Some(cap) = cap_def {
            for dep in &cap.dependencies {
                if let Dependency::Capability { id, .. } = dep {
                    self.dfs_cycles(&id, path, visited, cycles);
                }
            }
        }

        path.pop();
    }

    /// Return the topological sort of capability IDs for resolution.
    /// Returns an empty list if cycles are detected.
    pub fn topological_sort(&self) -> Result<Vec<String>, anyhow::Error> {
        let cycles = self.detect_cycles()?;
        if !cycles.is_empty() {
            anyhow::bail!("Cannot sort: circular dependency detected: {:?}", cycles);
        }

        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();

        self.topo_visit(&self.root_id, &mut visited, &mut result);

        Ok(result)
    }

    fn topo_visit(
        &self,
        id: &str,
        visited: &mut std::collections::HashSet<String>,
        result: &mut Vec<String>,
    ) {
        if visited.contains(id) {
            return;
        }

        visited.insert(id.to_string());

        let cap_def = registry::CAPABILITY_REGISTRY.get(id);
        if let Some(cap) = cap_def {
            for dep in &cap.dependencies {
                if let Dependency::Capability { id: dep_id, .. } = dep {
                    self.topo_visit(&dep_id, visited, result);
                }
            }
        }

        result.push(id.to_string());
    }

    /// Get all transitive dependencies for a capability.
    pub fn get_all_dependencies(&self) -> Vec<String> {
        let mut deps = Vec::new();
        let mut visited = std::collections::HashSet::new();
        self.collect_deps(&self.root_id, &mut visited, &mut deps);
        deps
    }

    fn collect_deps(
        &self,
        id: &str,
        visited: &mut std::collections::HashSet<String>,
        deps: &mut Vec<String>,
    ) {
        if visited.contains(id) {
            return;
        }

        visited.insert(id.to_string());

        let cap_def = registry::CAPABILITY_REGISTRY.get(id);
        if let Some(cap) = cap_def {
            for dep in &cap.dependencies {
                if let Dependency::Capability { id: dep_id, .. } = dep {
                    self.collect_deps(&dep_id, visited, deps);
                    if !deps.contains(dep_id) {
                        deps.push(dep_id.clone());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_cycles_for_single_capability() {
        let cap_def = registry::CAPABILITY_REGISTRY.get("docling").unwrap().clone();
        let graph = DependencyGraph::new("docling".to_string(), cap_def);
        let cycles = graph.detect_cycles().unwrap();
        assert!(cycles.is_empty());
    }

    #[test]
    fn test_topological_sort_single_capability() {
        let cap_def = registry::CAPABILITY_REGISTRY.get("docling").unwrap().clone();
        let graph = DependencyGraph::new("docling".to_string(), cap_def);
        let order = graph.topological_sort().unwrap();
        assert!(order.contains(&"docling".to_string()));
    }

    #[test]
    fn test_get_all_dependencies_docling() {
        let cap_def = registry::CAPABILITY_REGISTRY.get("docling").unwrap().clone();
        let graph = DependencyGraph::new("docling".to_string(), cap_def);
        let deps = graph.get_all_dependencies();
        // docling only has an ExternalTool dependency, no capability dependencies
        assert!(deps.is_empty());
    }

    #[test]
    fn test_get_all_dependencies_vision() {
        let cap_def = registry::CAPABILITY_REGISTRY.get("vision").unwrap().clone();
        let graph = DependencyGraph::new("vision".to_string(), cap_def);
        let deps = graph.get_all_dependencies();
        // vision has a Model dependency, no Capability dependency
        assert!(deps.is_empty());
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillDescriptor {
    pub id: String,
    pub description: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDescriptor {
    pub id: String,
    pub role: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRoster {
    pub agents: Vec<AgentDescriptor>,
}

pub fn built_in_skills() -> Vec<SkillDescriptor> {
    Vec::new()
}

pub fn built_in_agents() -> AgentRoster {
    AgentRoster {
        agents: vec![AgentDescriptor {
            id: "developer".to_string(),
            role: "implementation".to_string(),
            description: "Default LEAN implementation agent profile".to_string(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::{built_in_agents, built_in_skills};

    #[test]
    fn skills_catalog_serializes_as_array() {
        let value = serde_json::to_value(built_in_skills()).expect("skills should serialize");

        assert!(matches!(value, Value::Array(_)));
    }

    #[test]
    fn agent_roster_serializes_as_object() {
        let value = serde_json::to_value(built_in_agents()).expect("agents should serialize");

        assert!(matches!(value, Value::Object(_)));
        assert_eq!(
            value["agents"][0]["id"],
            Value::String("developer".to_string())
        );
    }
}

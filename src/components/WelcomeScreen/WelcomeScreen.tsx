import { Bot, Workflow } from "lucide-react";
import { useEffect, useState } from "react";
import {
  getAvailableSubAgents,
  getAvailableWorkflows,
  type SubAgentInfo,
  type WorkflowInfo,
} from "@/lib/ai";
import { cn } from "@/lib/utils";

function CategorySection({
  title,
  icon,
  children,
  color = "blue",
}: {
  title: string;
  icon: React.ReactNode;
  children: React.ReactNode;
  color?: "blue" | "purple" | "green";
}) {
  const colorClasses = {
    blue: "text-[var(--ansi-blue)] bg-[var(--ansi-blue)]/10",
    purple: "text-[var(--ansi-magenta)] bg-[var(--ansi-magenta)]/10",
    green: "text-[var(--ansi-green)] bg-[var(--ansi-green)]/10",
  };

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <div className={cn("p-1.5 rounded", colorClasses[color])}>{icon}</div>
        <h4 className="text-sm font-medium text-foreground">{title}</h4>
      </div>
      <div className="pl-8">{children}</div>
    </div>
  );
}

export function WelcomeScreen() {
  const [subAgents, setSubAgents] = useState<SubAgentInfo[]>([]);
  const [workflows, setWorkflows] = useState<WorkflowInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    async function fetchCapabilities() {
      try {
        const [subAgentsResult, workflowsResult] = await Promise.allSettled([
          getAvailableSubAgents(),
          getAvailableWorkflows(),
        ]);

        if (subAgentsResult.status === "fulfilled") {
          setSubAgents(subAgentsResult.value);
        }

        if (workflowsResult.status === "fulfilled") {
          setWorkflows(workflowsResult.value);
        }
      } catch (e) {
        setError(e instanceof Error ? e.message : "Failed to load capabilities");
      } finally {
        setLoading(false);
      }
    }

    fetchCapabilities();
  }, []);

  const hasCapabilities = subAgents.length > 0 || workflows.length > 0;

  return (
    <div className="flex flex-col items-center justify-center h-full text-muted-foreground p-8 overflow-auto">
      {/* Loading state */}
      {loading && <div className="text-sm text-muted-foreground">Loading capabilities...</div>}

      {/* Error state */}
      {error && !loading && <div className="text-sm text-[var(--ansi-red)]">{error}</div>}

      {/* Capabilities */}
      {!loading && !error && hasCapabilities && (
        <div className="w-full max-w-2xl space-y-6 text-left">
          {/* Sub-agents */}
          {subAgents.length > 0 && (
            <CategorySection title="Sub-Agents" icon={<Bot className="w-4 h-4" />} color="purple">
              <div className="space-y-2">
                {subAgents.map((agent) => (
                  <div key={agent.id} className="flex flex-col">
                    <span className="text-sm text-foreground">{agent.name}</span>
                    <span className="text-xs text-muted-foreground line-clamp-1">
                      {agent.description}
                    </span>
                  </div>
                ))}
              </div>
            </CategorySection>
          )}

          {/* Workflows */}
          {workflows.length > 0 && (
            <CategorySection
              title="Workflows"
              icon={<Workflow className="w-4 h-4" />}
              color="green"
            >
              <div className="space-y-2">
                {workflows.map((workflow) => (
                  <div key={workflow.name} className="flex flex-col">
                    <span className="text-sm text-foreground">{workflow.name}</span>
                    {workflow.description && (
                      <span className="text-xs text-muted-foreground line-clamp-1">
                        {workflow.description}
                      </span>
                    )}
                  </div>
                ))}
              </div>
            </CategorySection>
          )}
        </div>
      )}
    </div>
  );
}

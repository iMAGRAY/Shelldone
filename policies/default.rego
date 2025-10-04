# Shelldone UTIF-Σ Default Security Policy
# Evaluated by Σ-cap on agent.exec, agent.guard requests

package shelldone.policy

import rego.v1

# Default security level
default security_level := "hardened"

# OSC 52 clipboard policy
default allow_osc52_read := false
default allow_osc52_write := true

# Allowed ACK commands without approval
allowed_commands := {
    "agent.plan",
    "agent.exec",
    "agent.journal",
    "agent.inspect",
}

# Commands requiring explicit user approval
approval_required_commands := {
    "agent.guard",
    "agent.undo",
    "agent.connect",
}

# Allow basic commands in core/flux personas
allow if {
    input.persona in {"core", "flux"}
    input.command in allowed_commands
}

# Nova persona requires validation
allow if {
    input.persona == "nova"
    input.command in allowed_commands
    input.spectral_tag
}

# Guard commands always need approval
allow if {
    input.command in approval_required_commands
    input.approval_granted == true
}

# OSC sequences policy
allow_osc if {
    security_level == "hardened"
    input.osc_code in {0, 2, 4, 8, 133, 1337}
}

allow_osc if {
    security_level == "hardened"
    input.osc_code == 52
    input.operation == "write"
}

deny_reason contains msg if {
    not allow
    msg := sprintf("Policy denied: command=%v persona=%v", [input.command, input.persona])
}

deny_reason contains msg if {
    input.command in approval_required_commands
    not input.approval_granted
    msg := sprintf("Approval required for %v", [input.command])
}

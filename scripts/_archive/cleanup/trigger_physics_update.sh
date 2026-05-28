#!/bin/bash

# Trigger physics parameter update for VisionClaw
echo "Sending physics update to trigger ForceComputeActor..."

# Send update to trigger UpdateSimulationParams message
curl -X POST http://172.18.0.10:4000/api/settings \
  -H "Content-Type: application/json" \
  -d '{
    "visualisation": {
      "graphs": {
        "visionclaw": {
          "physics": {
            "springStrength": 5.0,
            "repulsionStrength": 50.0,
            "velocityDecay": 0.2
          }
        }
      }
    }
  }' 2>/dev/null | jq -r '.visualisation.graphs.visionclaw.physics' 2>/dev/null

echo "Physics update sent. Monitoring for UpdateSimulationParams in logs..."

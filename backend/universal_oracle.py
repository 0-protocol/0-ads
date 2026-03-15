import requests
import json
import datetime
from fastapi import FastAPI, HTTPException
from typing import Dict, Any, Callable

app = FastAPI()

# Registry of active Verification Graphs
# A Verification Graph takes a payload and returns a boolean True/False
VERIFICATION_REGISTRY: Dict[str, Callable[[Dict[str, Any]], bool]] = {}

def register_verifier(task_type: str):
    def decorator(func):
        VERIFICATION_REGISTRY[task_type] = func
        return func
    return decorator

# --- Pluggable Verifiers ---

@register_verifier("github_star")
def verify_github_star(payload: dict) -> bool:
    """
    Verifies if the agent's GitHub ID actually starred the target repo.
    """
    github_id = payload.get("agent_id")
    target_repo = payload.get("target")
    # Simulation: In production, checks GitHub API
    # e.g., requests.get(f"https://api.github.com/users/{github_id}/starred/{target_repo}")
    print(f"[Verifier: GitHub] Checking if {github_id} starred {target_repo}")
    return True

@register_verifier("moltbook_upvote")
def verify_moltbook_upvote(payload: dict) -> bool:
    """
    Verifies if the agent's Moltbook ID upvoted the target post.
    """
    agent_id = payload.get("agent_id")
    target_post = payload.get("target")
    # Simulation: In production, checks Moltbook API
    print(f"[Verifier: Moltbook] Checking if {agent_id} upvoted post {target_post}")
    return True

@register_verifier("twitter_retweet")
def verify_twitter_retweet(payload: dict) -> bool:
    """
    Verifies if the agent's Twitter ID retweeted the target tweet.
    """
    agent_id = payload.get("agent_id")
    target_tweet = payload.get("target")
    print(f"[Verifier: Twitter] Checking if @{agent_id} retweeted {target_tweet}")
    return True

@register_verifier("xiaohongshu_like")
def verify_xiaohongshu_like(payload: dict) -> bool:
    """
    Verifies if the agent liked a specific Xiaohongshu note.
    """
    agent_id = payload.get("agent_id")
    target_note = payload.get("target")
    print(f"[Verifier: Xiaohongshu] Checking if {agent_id} liked {target_note}")
    return True

# --- API Route ---

@app.post("/api/v2/oracle/verify")
async def verify_universal_claim(payload: dict):
    task_type = payload.get("task_type", "github_star")
    
    if task_type not in VERIFICATION_REGISTRY:
        raise HTTPException(
            status_code=400, 
            detail=f"Unsupported task_type: {task_type}. Supported: {list(VERIFICATION_REGISTRY.keys())}"
        )
        
    verifier = VERIFICATION_REGISTRY[task_type]
    
    # 1. Execute Modular Verification
    if not verifier(payload):
        raise HTTPException(
            status_code=403, 
            detail=f"Verification failed for task: {task_type}. Action not completed."
        )
        
    # 2. Sybil Defense Layer (Example call to anti_sybil checker)
    # check_anti_sybil(payload.get("agent_id"), task_type)
    
    # 3. Generate On-Chain Signature (Simulation)
    signature = "0xUniversalSignedProofOfIntent..."
    
    return {
        "status": "ok", 
        "task_type": task_type,
        "signature": signature
    }

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8080)

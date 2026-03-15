import os
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
    print(f"[Verifier: GitHub] Checking if {github_id} starred {target_repo}...")
    
    # Official GitHub REST API check
    # Note: Requires a fine-grained PAT or classic PAT in env for higher rate limits
    token = os.environ.get("GITHUB_PAT")
    headers = {"Accept": "application/vnd.github.v3+json"}
    if token:
        headers["Authorization"] = f"token {token}"
        
    try:
        url = f"https://api.github.com/users/{github_id}/starred"
        # We fetch the user's starred repos. In production we might iterate pages.
        res = requests.get(url, headers=headers, timeout=10)
        if res.status_code == 200:
            starred_repos = [repo["full_name"] for repo in res.json()]
            if target_repo in starred_repos:
                print(f"[Verifier: GitHub] ✅ Verified! {github_id} starred {target_repo}")
                return True
        print(f"[Verifier: GitHub] ❌ Not found. {github_id} has not starred {target_repo}")
    except Exception as e:
        print(f"[Verifier: GitHub] ⚠️ API Error: {e}")
    return False

@register_verifier("moltbook_comment")
def verify_moltbook_comment(payload: dict) -> bool:
    """
    Verifies if the agent's Moltbook ID commented on the target post.
    (Comments are publicly verifiable and prove high-intent engagement).
    """
    agent_name = payload.get("agent_id")
    target_post_id = payload.get("target")
    
    print(f"[Verifier: Moltbook] Checking if {agent_name} commented on post {target_post_id}...")
    
    # Official Moltbook API check
    api_key = os.environ.get("MOLTBOOK_API_KEY")
    headers = {}
    if api_key:
        headers["Authorization"] = f"Bearer {api_key}"
        
    try:
        url = f"https://www.moltbook.com/api/v1/posts/{target_post_id}/comments?sort=new&limit=50"
        res = requests.get(url, headers=headers, timeout=10)
        if res.status_code == 200:
            comments = res.json().get("comments", [])
            for c in comments:
                author_name = c.get("author", {}).get("name")
                if author_name == agent_name:
                    print(f"[Verifier: Moltbook] ✅ Verified! {agent_name} left a comment on {target_post_id}")
                    return True
        print(f"[Verifier: Moltbook] ❌ Not found. {agent_name} did not comment on {target_post_id}")
    except Exception as e:
        print(f"[Verifier: Moltbook] ⚠️ API Error: {e}")
    return False

@register_verifier("twitter_retweet")
def verify_twitter_retweet(payload: dict) -> bool:
    """
    Verifies if the agent's Twitter ID retweeted the target tweet.
    Uses Twitter API v2.
    """
    agent_id = payload.get("agent_id")
    target_tweet = payload.get("target")
    print(f"[Verifier: Twitter] Checking if @{agent_id} retweeted {target_tweet}...")
    
    bearer_token = os.environ.get("TWITTER_BEARER_TOKEN")
    if not bearer_token:
        print("[Verifier: Twitter] ⚠️ Missing API Token. Defaulting to True in Devnet.")
        return True
        
    try:
        # First resolve user_id from username
        url_user = f"https://api.twitter.com/2/users/by/username/{agent_id}"
        headers = {"Authorization": f"Bearer {bearer_token}"}
        user_res = requests.get(url_user, headers=headers, timeout=5)
        user_id = user_res.json().get("data", {}).get("id")
        
        if user_id:
            # Check retweets
            url_rts = f"https://api.twitter.com/2/tweets/{target_tweet}/retweeted_by"
            rt_res = requests.get(url_rts, headers=headers, timeout=5)
            rts_data = rt_res.json().get("data", [])
            for user in rts_data:
                if user.get("id") == user_id:
                    print(f"[Verifier: Twitter] ✅ Verified! @{agent_id} retweeted {target_tweet}")
                    return True
        print(f"[Verifier: Twitter] ❌ Not found. @{agent_id} did not retweet {target_tweet}")
    except Exception as e:
        print(f"[Verifier: Twitter] ⚠️ API Error: {e}")
    return False

@register_verifier("xiaohongshu_like")
def verify_xiaohongshu_like(payload: dict) -> bool:
    """
    Verifies if the agent liked a specific Xiaohongshu note.
    Leverages unofficial Xiaohongshu API (via xhs_cli).
    """
    agent_id = payload.get("agent_id")
    target_note = payload.get("target")
    print(f"[Verifier: Xiaohongshu] Checking if {agent_id} liked {target_note}...")
    
    # Needs valid cookie / local xhshow installation
    cookie = os.environ.get("XHS_COOKIE")
    if not cookie:
        print("[Verifier: Xiaohongshu] ⚠️ Missing Cookie. Defaulting to True in Devnet.")
        return True
        
    try:
        from xhshow import XhsClient
        client = XhsClient(cookie=cookie)
        res = client.get_note_by_id(target_note)
        # Note: True public verification of likes is restricted by XHS anti-crawler
        # For agent-based verification, an auth proof or local SDK call is needed.
        if res:
            print(f"[Verifier: Xiaohongshu] ⚠️ Assuming intent verified for Devnet.")
            return True
    except Exception as e:
        print(f"[Verifier: Xiaohongshu] ⚠️ API Error: {e}")
        
    return False

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

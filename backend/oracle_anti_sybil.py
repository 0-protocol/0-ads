import requests
import json
import datetime
from fastapi import FastAPI, HTTPException

app = FastAPI()

# Minimum Account Age (e.g. 1 year)
MIN_ACCOUNT_AGE_DAYS = 365
# Minimum Followers
MIN_FOLLOWERS = 10

def check_github_sybil(github_id: str) -> bool:
    """
    Returns True if the account passes anti-sybil checks, False otherwise.
    """
    url = f"https://api.github.com/users/{github_id}"
    try:
        response = requests.get(url, timeout=5)
        if response.status_code != 200:
            return False
            
        user_data = response.json()
        followers = user_data.get("followers", 0)
        created_at_str = user_data.get("created_at")
        
        if not created_at_str:
            return False
            
        created_at = datetime.datetime.strptime(created_at_str, "%Y-%m-%dT%H:%M:%SZ")
        age_days = (datetime.datetime.utcnow() - created_at).days
        
        # Rule 1: Account must be older than 1 year OR have significant followers
        if age_days < MIN_ACCOUNT_AGE_DAYS and followers < MIN_FOLLOWERS:
            print(f"[Anti-Sybil] Rejected {github_id}: Age {age_days}d, Followers {followers}")
            return False
            
        print(f"[Anti-Sybil] Passed {github_id}: Age {age_days}d, Followers {followers}")
        return True
        
    except Exception as e:
        print(f"[Anti-Sybil] API Error for {github_id}: {e}")
        return False

@app.post("/api/v1/oracle/verify")
async def verify_claim(payload: dict):
    github_id = payload.get("agent_github_id")
    
    # Sybil Defense Layer
    if not check_github_sybil(github_id):
        raise HTTPException(
            status_code=403, 
            detail="Account flagged by Anti-Sybil system. Account too new or insufficient reputation."
        )
        
    # (Rest of Oracle signing logic goes here...)
    return {"status": "ok", "signature": "0x..."}

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8080)

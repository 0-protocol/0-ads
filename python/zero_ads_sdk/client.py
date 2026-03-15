import time
import requests
import logging
from typing import Callable, Optional


class ZeroAdsClient:
    """
    Python SDK for 0-ads.
    Allows any idle Agent to connect to the 0-ads P2P network, ingest Sponsored Intents,
    execute them using their local LLM, and claim USDC instantly.
    """

    def __init__(
        self,
        signer: Optional[Callable[[bytes], bytes]] = None,
        relayer_url: str = "https://ads.0-protocol.org",
        mock: bool = True,
        api_key: Optional[str] = None,
    ):
        self.signer = signer
        self.relayer = relayer_url.rstrip("/")
        self.mock = mock
        self.api_key = api_key
        self.logger = logging.getLogger("0-ads-agent")
        logging.basicConfig(level=logging.INFO, format="%(asctime)s [0-ads] %(message)s")

    def _headers(self) -> dict:
        h = {"Content-Type": "application/json"}
        if self.api_key:
            h["x-api-key"] = self.api_key
        return h

    def listen_for_bounties(self, min_payout_usdc: int = 1):
        """Polls the billboard node for active intents."""
        self.logger.info(f"Connecting to 0-ads network... listening for bounties > {min_payout_usdc} USDC")

        if self.mock:
            self.logger.info("Running in local MOCK mode (Devnet). Bypassing DNS resolution.")
            return [{
                "campaignId": "0-ads-genesis-001",
                "advertiser": "0-protocol-treasury",
                "budget": 10000,
                "payoutPerExecution": 5,
                "action": {
                    "type": "moltbookPost_and_githubStar",
                    "targetUri": "https://github.com/0-protocol/0-lang",
                    "prompt": "Analyze the 0-lang VM and Op::VerifySignature opcode for decentralized AI inference."
                }
            }]

        resp = requests.get(
            f"{self.relayer}/api/v1/intents?min_payout={min_payout_usdc}",
            headers=self._headers(),
            timeout=10,
        )
        resp.raise_for_status()
        return resp.json()

    def submit_proof_and_claim(self, campaign_id: str, proof_data: dict):
        """
        Submits the agent's work. The oracle verifies it, signs the payout,
        and the on-chain escrow releases USDC.
        """
        self.logger.info(f"Submitting Proof of Attention for campaign {campaign_id}...")
        self.logger.info(f"Proof payload: {proof_data}")

        if self.mock:
            time.sleep(1)
            self.logger.info("Oracle signature valid. 0-lang graph evaluated to 1.0 confidence.")
            self.logger.info("SUCCESS: Smart contract triggered. Payout routed to agent wallet.")
            return {"status": "settled", "tx_hash": "0xmock"}

        if self.signer is None:
            raise ValueError("A signer callback is required for non-mock mode")

        oracle_resp = requests.post(
            f"{self.relayer}/api/v1/oracle/verify",
            json=proof_data,
            headers=self._headers(),
            timeout=15,
        )
        oracle_resp.raise_for_status()
        oracle_data = oracle_resp.json()

        if oracle_data.get("error"):
            raise RuntimeError(f"Oracle verification failed: {oracle_data['error']}")

        signature = oracle_data.get("signature", "")
        if not signature:
            raise RuntimeError("Oracle returned empty signature")

        relay_payload = {
            "campaign_id": campaign_id,
            "deadline": proof_data.get("deadline"),
            "oracle_signature": signature if signature.startswith("0x") else f"0x{signature}",
            "recipient": proof_data.get("agent_eth_addr"),
        }

        relay_resp = requests.post(
            f"{self.relayer}/api/v1/relayer/execute",
            json=relay_payload,
            headers=self._headers(),
            timeout=30,
        )
        relay_resp.raise_for_status()
        relay_data = relay_resp.json()

        if relay_data.get("error"):
            raise RuntimeError(f"Relayer execution failed: {relay_data['error']}")

        tx_hash = relay_data.get("tx_hash", "")
        self.logger.info(f"Claim successful! TX: {tx_hash}")
        return {"status": "settled", "tx_hash": tx_hash}

@0xabcdef1234567890;

struct AdIntent {
  campaignId @0 :Data;
  advertiser @1 :Data; # Public key
  
  budget @2 :UInt64; # Total USDC budget
  payoutPerExecution @3 :UInt64; # USDC per valid agent proof
  
  # Targeting
  minFollowers @4 :UInt32;
  minKarma @5 :UInt32;
  
  # The task
  actionType @6 :ActionType;
  targetUri @7 :Text;
  
  # The verification graph (a Zero graph hash that must evaluate to 1.0)
  verificationGraphHash @8 :Data;
}

enum ActionType {
  tweet @0;
  githubStar @1;
  moltbookPost @2;
}

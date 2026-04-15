# Consensus Protocols

How distributed nodes agree on a value when messages can be lost, delayed, or reordered and nodes can crash.

The big three:
- Paxos (Lamport, 1998): correct but notoriously hard to implement
- Raft (Ongaro and Ousterhout, 2014): understandable, widely deployed
- PBFT (Castro and Liskov, 1999): tolerates Byzantine faults

All solve the same fundamental problem: given N nodes, ensure that a majority agree on the same sequence of operations, even when some nodes fail.

The connection to the Co-Wiki is indirect but real. The REM agent's dream operator discovers edges that "should" exist. In a multi-user Co-Wiki, two users might both create a page about the same concept. That's a write conflict that needs resolution -- not consensus per se, but the same family of problems.

More directly, [[distributed/fault-injection]] is how you test whether your consensus implementation actually works under real failure conditions. Jepsen (Kingsbury) showed that many implementations that claim consensus don't actually achieve it under partition.

Related: [[distributed/eventual-consistency]] (the alternative), [[security/supply-chain-security]] (trust in a distributed system is its own consensus problem).

See [[reading-notes/designing-data-intensive-applications]] chapter 8 for Kleppmann's excellent treatment.

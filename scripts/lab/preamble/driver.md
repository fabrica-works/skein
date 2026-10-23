Carry out the charter below. Use commands under the lab permission profile and
only the experiment's tools on the lab MCP server. Each tool takes args (an array
of strings) and stdin (text or null). Work in the supplied working directory;
the source is read-only and the network is unavailable to commands. Tool calls
have their own declared read and write roots. The runner records actions and
stops the drive when its bounds pass.

Your final message is one JSON object: done (boolean), retried (array of
{step, tries: [{tried, why}]}), unseen (array of {step, looked_for}), and
surprises (array of strings). Set done only when you reached the charter's goal.
Describe retries and steps you could not see how to take.

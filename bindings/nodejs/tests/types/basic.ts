import { Client } from "../../index";

async function checkPublicIterator(): Promise<void> {
  const connection = await new Client("lake://example:443/default").getConn();
  const rows = await connection.queryIter("SELECT 1");
  for await (const row of rows) {
    row.values();
  }
  await connection.close();
}

void checkPublicIterator;

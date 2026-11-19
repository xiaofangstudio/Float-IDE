import { readFileSync } from "node:fs";
const lines = readFileSync("src-tauri/float-lang/bootstrap/colang.co","utf8").split("\n");
for (const ln of [3209,3210,3211]) {
  const s = lines[ln-1];
  console.log(ln, "len=", s.length, JSON.stringify(s.slice(60,100)));
}
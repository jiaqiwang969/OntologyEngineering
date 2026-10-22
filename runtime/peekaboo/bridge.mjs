// Thin Peekaboo MCP transport. Rust owns task, action, account and supplier policy.
// Reuses misumi-incad's existing-background-page and pinned-provider mechanism.
import { spawn, execFileSync } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createHash, randomUUID } from "node:crypto";

const [socketPath, peekaboo, receiptDirectory, cdpEndpoint] = process.argv.slice(2);
if (!socketPath || !path.isAbsolute(socketPath) || !peekaboo || !receiptDirectory) throw new Error("Explicit socket, Peekaboo and owned receipt directory required");
if (fs.existsSync(socketPath)) throw new Error("Existing socket is never replaced");
if(!cdpEndpoint)throw new Error("Explicit Rust-verified existing CDP endpoint required; profile discovery is forbidden");
const endpoint=new URL(cdpEndpoint);
if(endpoint.protocol!=="http:" || !["127.0.0.1","[::1]"].includes(endpoint.hostname) ||  endpoint.username || endpoint.password || endpoint.pathname!=="/" || endpoint.search || endpoint.hash)throw new Error("Explicit loopback CDP endpoint required");
const here = path.dirname(fileURLToPath(import.meta.url));
const protocol = "oev2.peekaboo-bridge/1";
const lock = JSON.parse(fs.readFileSync(path.join(here,"provider-lock.json"),"utf8"));
const digest = bytes => createHash("sha256").update(bytes).digest("hex");
const bridgeSha = digest(fs.readFileSync(fileURLToPath(import.meta.url)));
const hookSha = digest(fs.readFileSync(path.join(here,"focus-policy.mjs")));
const pinFile = path.join(receiptDirectory,"provider-loaded.json");
const binarySha = digest(fs.readFileSync(fs.realpathSync(peekaboo)));
if(binarySha !== lock.peekaboo_binary_sha256) throw new Error("Peekaboo binary pin changed");
const state = {status:"worker_started",worker_pid:process.pid,worker_session:randomUUID(),chrome_pid:null,
  bridge_sha256:bridgeSha,focus_hook_sha256:hookSha,peekaboo_binary_sha256:binarySha,
  chrome_devtools_source_sha256:lock.chrome_devtools_source_sha256,provider_pin_applied:false,
  cdp_endpoint_sha256:digest(cdpEndpoint),error_stage:null,
  provider_page_ids_changed:false,
  chrome_launches:0,settings_changes:0,focus_actions:0,storage_reads:0,request_retries:0};
function apps() {
  if(digest(fs.readFileSync(fs.realpathSync(peekaboo))) !== lock.peekaboo_binary_sha256)throw new Error("Peekaboo binary pin changed");
  const raw = JSON.parse(execFileSync(peekaboo,["list","apps","--json","--no-remote"],{encoding:"utf8",maxBuffer:8*1024*1024,timeout:5000}));
  if(raw.success !== true || !Array.isArray(raw.data?.applications)) throw new Error("Peekaboo applications unavailable");
  const active = raw.data.applications.filter(a=>a.isActive);
  const chrome = raw.data.applications.filter(a=>a.bundleIdentifier === "com.google.Chrome");
  if(active.length !== 1 || chrome.length !== 1) throw new Error("Application or existing Chrome identity ambiguous");
  return {active:{bundle:active[0].bundleIdentifier,pid:active[0].processIdentifier},chrome_pid:chrome[0].processIdentifier};
}
const same = (a,b) => a?.bundle === b?.bundle && a?.pid === b?.pid;
function providerPin() {
  const pin = JSON.parse(fs.readFileSync(pinFile,"utf8"));
  if(typeof pin.source_path!=="string" || !pin.source_path.endsWith("/chrome-devtools-mcp/build/src/McpPage.js"))throw new Error("Provider source path identity changed");
  const original=fs.readFileSync(pin.source_path,"utf8");
  const needle="this.pptrPage.emulateFocusedPage(true)";
  if(pin.original_sha256 !== lock.chrome_devtools_source_sha256 || pin.synthetic_focus_enabled !== false ||
     digest(original) !== pin.original_sha256 || original.split(needle).length!==2 ||
     digest(original.replace(needle,"this.pptrPage.emulateFocusedPage(false)"))!==pin.patched_sha256) throw new Error("Provider source pin changed or focus patch unverified");
  const root=path.resolve(path.dirname(pin.source_path),"../..");
  for(const [relative,expected] of Object.entries(lock.provider_files)){
    if(digest(fs.readFileSync(path.join(root,relative)))!==expected)throw new Error("Provider source pin changed");
  }
  state.provider_pin_applied = true;
}
function value(result) {
  if (result.structuredContent && Object.hasOwn(result.structuredContent,"result")) return result.structuredContent.result;
  for (const item of result.content ?? []) {
    if(item.type !== "text") continue;
    const block=item.text.match(/```json\s*([\s\S]*?)\s*```/);
    if(block) return JSON.parse(block[1]);
  }
  throw new Error("No structured page evaluation result");
}
const transportEnvironment={...process.env,NODE_OPTIONS:`--import=${path.join(here,"focus-policy.mjs")}`,OEV2_PEEKABOO_PROVIDER_RECEIPT:pinFile,npm_config_offline:"true"};
delete transportEnvironment.NPM_CONFIG_OFFLINE;
for(const key of ["PEEKABOO_BROWSER_MCP_BROWSER_URL","PEEKABOO_BROWSER_MCP_ISOLATED","PEEKABOO_BROWSER_MCP_HEADLESS"])delete transportEnvironment[key];
transportEnvironment.PEEKABOO_BROWSER_MCP_BROWSER_URL=cdpEndpoint;
const child=spawn(peekaboo,["mcp","--no-remote"],{stdio:["pipe","pipe","ignore"],
  env:transportEnvironment});
let serial=0,buffer="",closed=false;
const pending=new Map();
child.stdout.setEncoding("utf8");
child.stdout.on("data",chunk=>{
  buffer+=chunk;
  if(buffer.length>40*1024*1024){child.kill("SIGTERM");state.status="outcome_unknown";return}
  for(;;){
    const end=buffer.indexOf("\n");if(end<0)break;
    const line=buffer.slice(0,end);buffer=buffer.slice(end+1);
    let message;try{message=JSON.parse(line)}catch{continue}
    const task=pending.get(message.id);if(!task)continue;pending.delete(message.id);clearTimeout(task.timer);
    message.error?task.reject(new Error("Peekaboo RPC error")):task.resolve(message.result);
  }
});
child.on("exit",()=>{closed=true;state.status="closed";for(const task of pending.values()){clearTimeout(task.timer);task.reject(new Error("Peekaboo transport closed"))}pending.clear()});
function rpc(method,params) {
  if(closed) return Promise.reject(new Error("Peekaboo transport closed"));
  const id=++serial;
  return new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>{pending.delete(id);state.status="outcome_unknown";reject(new Error("Peekaboo request outcome unknown; no retry"))},60000);
    pending.set(id,{resolve,reject,timer});
    child.stdin.write(JSON.stringify({jsonrpc:"2.0",id,method,params})+"\n");
  });
}
async function browser(args) {
  state.error_stage=`browser_${args.action}`;
  const result=await rpc("tools/call",{name:"browser",arguments:args});
  if((result.content??[]).some(c=>c.type==="text" && /browser was restarted or reconnected[\s\S]*Page ids have changed/.test(c.text)))state.provider_page_ids_changed=true;
  if(result.isError)throw new Error("Peekaboo browser operation rejected or unconfirmed");
  return result;
}
const initialized=(async()=>{
  await rpc("initialize",{protocolVersion:"2024-11-05",capabilities:{},clientInfo:{name:"oev2-peekaboo-transport",version:"1"}});
  child.stdin.write(JSON.stringify({jsonrpc:"2.0",method:"notifications/initialized"})+"\n");
})();
initialized.catch(()=>{});
let baseline=null;
const owned=new Map(),bound=new Map();
async function evaluate(pageId,fn) {
  return value(await browser({action:"call",mcp_tool:"evaluate_script",mcp_args_json:JSON.stringify({pageId,function:fn,dialogAction:"",waitForStableDom:false})}));
}
const literal=v=>JSON.stringify(v).replace(/</g,"\\u003c");
async function target(pageId,expectedUrl,exact=false) {
  const observed=await evaluate(pageId,"() => ({url:location.href,origin:location.origin,page_has_focus:document.hasFocus()})");
  return observed;
}
async function connectExisting(before) {
  await initialized;
  if(state.status === "outcome_unknown" || closed)throw new Error("Transport state unresolved; no automatic reconnect");
  if(!baseline) {
    const raw=await browser({action:"status"});
    const text=(raw.content??[]).filter(c=>c.type==="text").map(c=>c.text).join("\n");
    if(!/Detected Chrome:\s*\n- /.test(text))throw new Error("Existing Chrome not detected");
    await browser({action:"connect",channel:"stable"});
  }
  const pages=await browser({action:"list_pages"}); // Actual browser operation, separate from MCP startup.
  providerPin();
  const after=apps();
  baseline=after.active;state.chrome_pid=after.chrome_pid;state.status="ready";
  const supplierPages=(pages.content??[]).filter(c=>c.type==="text").flatMap(c=>[...c.text.matchAll(/^(\d+):\s*(https?:\/\/\S+)/gm)]).flatMap(m=>{
    try {const u=new URL(m[2]);return u.hostname==="www.misumi.com.cn"?[{page_id:Number(m[1]),page_origin:u.origin,page_path:u.pathname}]:[]}catch{return []}
  });
  return {connection_verified:true,real_operation:"list_pages",supplier_pages:supplierPages};
}
async function operation(command) {
  if(command.schema!==protocol)throw new Error("Bridge protocol mismatch");
  const before=apps();
  let result;
  if(command.action==="status") return {schema:protocol,success:state.status==="ready",...state};
  if(command.action==="check") result=await connectExisting(before);
  else {
    if(state.status!=="ready" || !baseline)throw new Error("Verified existing connection required");
    providerPin();
    if(command.worker_session!==state.worker_session)throw new Error("Worker session changed");
    if(command.action==="open") {
      if(typeof command.owner!=="string" || !command.owner)throw new Error("Owned target token required");
      const u=new URL(command.url);if(!["http:","https:"].includes(u.protocol)||u.username||u.password)throw new Error("Public page URL required");
      const listing=await browser({action:"list_pages"});
      const prior=(listing.content??[]).filter(c=>c.type==="text").flatMap(c=>[...c.text.matchAll(/^(\d+):/gm)]).map(m=>Number(m[1]));
      const created=await browser({action:"new_page",url:command.url,background:true});
      const text=(created.content??[]).filter(c=>c.type==="text").map(c=>c.text).join("\n");
      const selected=[...text.matchAll(/^(\d+):[^\n]*\[selected\]/gm)];
      if(selected.length!==1)throw new Error("Created background target identity unresolved");
      const id=Number(selected[0][1]);
      if(prior.includes(id))throw new Error("Created target was already present");
      owned.set(id,command.owner);bound.set(id,command.url);
      result={page_id:id,owns_target:true,prior_page_ids:prior,selected_created_page_id:id};
    } else if(command.action==="bind") {
      if(!Number.isSafeInteger(command.page_id)||command.page_id<=0)throw new Error("Explicit target required");
      bound.set(command.page_id,command.url);
      result={page_id:command.page_id,owns_target:false};
    } else {
      const scope=bound.get(command.page_id);if(!scope)throw new Error("Bound target required");
      if(command.action==="target_state") {
        result=await target(command.page_id,scope);
      } else if(command.action==="evaluate") {
        if(typeof command.expression!=="string"||command.expression.length>60000)throw new Error("Bounded code-owned expression required");
        const fn=`() => (${command.expression})`;
        result={value:await evaluate(command.page_id,fn)};
      } else if(command.action==="screenshot") {
        const shot=await browser({action:"call",mcp_tool:"take_screenshot",mcp_args_json:JSON.stringify({pageId:command.page_id,format:"jpeg",quality:72,fullPage:false})});
        const image=(shot.content??[]).find(c=>c.type==="image");if(!image?.data)throw new Error("Target screenshot unconfirmed");
        result={data:image.data};
      } else if(command.action==="navigate") {
        const u=new URL(command.url);if(!["http:","https:"].includes(u.protocol)||u.username||u.password)throw new Error("Public navigation URL required");
        await browser({action:"call",mcp_tool:"navigate_page",mcp_args_json:JSON.stringify({pageId:command.page_id,url:command.url,type:"url",timeout:30000})});
        result={navigated:true};
      } else if(command.action==="close") {
        if(owned.get(command.page_id)!==command.owner)throw new Error("Only this owner's background target may close");
        await browser({action:"close_page",page_id:command.page_id});owned.delete(command.page_id);bound.delete(command.page_id);result={closed:true};
      } else throw new Error("Unsupported transport operation");
    }
  }
  const targetId=command.action==="open"?result.page_id:command.page_id;
  const targetAfter=command.action!=="close" && Number.isInteger(targetId)?await target(targetId,bound.get(targetId)):null;
  const after=apps();
  return {schema:protocol,success:true,...state,focus_before:before.active,focus_after:after.active,
    active_application_unchanged:same(before.active,after.active),chrome_pid_before:before.chrome_pid,chrome_pid_after:after.chrome_pid,target_after:targetAfter,result};
}
let queue=Promise.resolve();
const server=net.createServer(socket=>{
  socket.setEncoding("utf8");let incoming="",received=false;
  socket.on("data",chunk=>{
    if(received){socket.destroy();return}
    incoming+=chunk;if(incoming.length>65536){socket.destroy();return}
    const end=incoming.indexOf("\n");if(end<0)return;
    received=true;
    if(end+1!==incoming.length){socket.destroy();return}
    let command;try{command=JSON.parse(incoming.slice(0,end))}catch{socket.destroy();return}
    const run=async()=>{
      if(socket.destroyed)return;
      let result;
      try{result=await operation(command)}catch(error){
        const detail=String(error.message);
        const code=/pin|source path identity|focus patch/.test(detail)?"peekaboo_provider_identity_changed":
          /Application|application/.test(detail)?"peekaboo_active_application_changed":
          /page focus/.test(detail)?"peekaboo_target_focus_changed":
          /source changed/.test(detail)?"peekaboo_target_source_changed":
          /outcome unknown|state unresolved/.test(detail)?"peekaboo_request_outcome_unknown":
          /Worker session|Chrome instance/.test(detail)?"peekaboo_session_identity_changed":"peekaboo_operation_unconfirmed";
        result={schema:protocol,success:false,...state,error_code:code,request_retries:0};
      }
      const raw=JSON.stringify(result);
      if(raw.length>32*1024*1024){socket.end(JSON.stringify({schema:protocol,success:false,error_code:"response_limit_exceeded"})+"\n");return}
      socket.end(raw+"\n");
    };
    queue=queue.then(run,run);
  });
});
server.listen(socketPath,()=>fs.chmodSync(socketPath,0o600));
function cleanup(){child.kill("SIGTERM");server.close();try{fs.unlinkSync(socketPath)}catch{}process.exit(0)}
process.on("SIGTERM",cleanup);process.on("SIGINT",cleanup);

"""Attach to an observed Chrome tab without creating/activating any tab.

Uses only normal page DOM through a previously authorized Apple Events setting.
It never changes that setting, reads browser storage, or launches Chrome. The
unmodified upstream snapshot/decision code is reused; DOM input is a different
executor from Browser Harness and is recorded as such.
"""
from __future__ import annotations

import json
from pathlib import Path
import subprocess
import time
from urllib.parse import urlsplit


def evaluate_tab(tab_id, expression, origin, *, runner=subprocess.run):
    if type(tab_id) is not int or tab_id <= 0:
        raise ValueError("observed_numeric_tab_id_required")
    script = "(() => { if (location.origin !== " + json.dumps(origin) + ") "
    script += "throw new Error('Unexpected target origin'); return JSON.stringify((" + expression + ")); })()"
    apple = '''if application "Google Chrome" is not running then error "Chrome is not running"
tell application "Google Chrome"
 repeat with w in windows
  repeat with t in tabs of w
   if (id of t as text) is TAB_ID then return execute t javascript JS_SOURCE
  end repeat
 end repeat
 error "Observed target tab no longer exists"
end tell
'''.replace("TAB_ID", json.dumps(str(tab_id))).replace("JS_SOURCE", json.dumps(script, ensure_ascii=False))
    result = runner(["osascript", "-"], input=apple, text=True, capture_output=True, timeout=30)
    if result.returncode:
        # No page contents, script source or system error string enters diagnostics.
        raise RuntimeError("apple_events_execution_unconfirmed")
    return json.loads(result.stdout)


class AppleEventsBrowser:
    def __init__(self, url, task):
        from jev_ultrafast.browser import READ_STATE, StalePage
        self._snapshot = READ_STATE
        selectors = task.get("custom_click_selectors", [])
        if selectors:
            # Caller supplies selectors established by current page observation.
            # Do not change the DOM's roles, labels or event handlers.
            self._snapshot = "(() => { const p=" + READ_STATE + "; if(!p)return null; " + """
              const c=window.__jevFast, extra=[];
              for(const selector of SELECTORS) for(const e of document.querySelectorAll(selector)) {
                if(!e.isConnected || !e.checkVisibility({checkOpacity:true,checkVisibilityCSS:true}) ||
                   e.matches(':disabled') || e.closest('[aria-disabled="true"],[inert]')) continue;
                const r=e.getBoundingClientRect(),x=r.x+r.width/2,y=r.y+r.height/2;
                if(!r.width||!r.height||x<0||y<0||x>=innerWidth||y>=innerHeight)continue;
                if(!c.ids.has(e))c.ids.set(e,c.next++);
                const node=c.ids.get(e); c.nodes.set(node,e);
                if(p.actions.some(a=>a.node===node))continue;
                const label=e.innerText.trim().replace(/\\s+/g,' ');
                if(!label)continue;
                const a={id:'custom_'+node,node,kind:'click',role:'button',label,
                  value:'',rect:{x:r.x,y:r.y,w:r.width,h:r.height}};
                extra.push(a);p.guards[node]=c.guard(e);
              }
              p.actions.push(...extra);p.marker.push(extra.map(({rect,...a})=>a));return p;
            })()""".replace("SELECTORS", json.dumps(selectors))
        self._stale = StalePage
        self.target = task["tab_id"]
        self.session = "chrome-apple-events-existing-tab"
        self.origin = urlsplit(url)._replace(path="", query="", fragment="").geturl()
        self.allowed = task["allowed_actions"]
        self.execution_kind = "chrome_apple_events_dom"
        self.owns_tab = False
        current = self.evaluate("location.href")
        if current != url:
            raise ValueError("existing_tab_url_mismatch")

    def evaluate(self, expression):
        return evaluate_tab(self.target, expression, self.origin)

    def permitted(self, action):
        if action["kind"] in {"scroll", "wait"}:
            return True
        return any(rule["kind"] == action["kind"] and rule["label"] == action["label"]
                   for rule in self.allowed)

    def observe(self, screenshot=False):
        from jev_ultrafast.browser import fingerprint
        if screenshot:
            raise ValueError("apple_events_has_no_screenshot_capture")
        page = self.evaluate(self._snapshot)
        if page is None:
            raise self._stale("Page has no body")
        page["actions"] = [a for a in page["actions"] if self.permitted(a)]
        # Upstream visible text omits native SELECT's displayed selected option.
        # Preserve that readback even when the chosen value has no remaining action.
        page["selected_values"] = self.evaluate("[...document.querySelectorAll('select')]"
            ".filter(e=>e.checkVisibility({checkOpacity:true,checkVisibilityCSS:true}))"
            ".map(e=>({id:e.id,value:e.value,label:[...e.selectedOptions].map(o=>o.label).join(', ')}))")
        if page["selected_values"]:
            page["text"] += "\nObserved selected dropdown values: " + json.dumps(page["selected_values"], ensure_ascii=False)
        page["fingerprint"] = fingerprint(page)
        return page

    def fresh(self, page, action=None):
        if action and action["kind"] in {"click", "select", "fill"}:
            if type(action.get("node")) is not int:
                return False
            current = self.evaluate("(() => { const c=window.__jevFast; return c ? "
                + "[c.pageKey(),c.guard(c.nodes.get(" + str(action["node"]) + "))] : null; })()")
            return current == [page["page_key"], page["guards"].get(str(action["node"]))]
        marker = self.evaluate("(() => { const s=" + self._snapshot + "; return s?.marker ?? null; })()")
        return marker == page["marker"]

    def act(self, action, page, text=None):
        if not self.permitted(action):
            raise ValueError("action_outside_declared_scope")
        if not self.fresh(page, action):
            raise self._stale("Page changed before action")
        if action["kind"] == "wait":
            time.sleep(0.25)
            return {"executed": action["id"]}
        data = {"action": action, "text": text, "page_key": page["page_key"],
                "guard": page["guards"].get(str(action.get("node")))}
        result = self.evaluate("""(p => {
          const a=p.action,c=window.__jevFast;
          if (!c || JSON.stringify(c.pageKey())!==JSON.stringify(p.page_key)) return {stale:true};
          if (a.kind==='scroll') { window.scrollBy(0,a.delta); return {executed:a.id}; }
          const e=c.nodes.get(a.node);
          if (!e?.isConnected || JSON.stringify(c.guard(e))!==JSON.stringify(p.guard) ||
              e.matches(':disabled') || e.closest('[aria-disabled="true"],[inert]') ||
              ['password','file','hidden'].includes(e.type) ||
              !e.checkVisibility({checkOpacity:true,checkVisibilityCSS:true})) return {stale:true};
          const r=e.getBoundingClientRect(),x=r.x+r.width/2,y=r.y+r.height/2;
          if (!r.width || !r.height || x<0 || y<0 || x>=innerWidth || y>=innerHeight ||
              !e.contains(document.elementFromPoint(x,y))) return {stale:true};
          if (a.kind==='select') {
            if (e.tagName!=='SELECT' || ![...e.options].some(o=>o.value===a.value &&
                !o.disabled && !o.closest('optgroup[disabled]'))) return {stale:true};
            e.value=a.value;
            e.dispatchEvent(new Event('input',{bubbles:true}));
            e.dispatchEvent(new Event('change',{bubbles:true}));
            return {executed:a.id,readback:e.value,expected:a.value};
          }
          if (a.kind==='fill') {
            if (e.readOnly || e.getAttribute('aria-readonly')==='true' ||
                typeof p.text!=='string') return {stale:true};
            const proto=e.tagName==='INPUT' ? HTMLInputElement.prototype :
                        e.tagName==='TEXTAREA' ? HTMLTextAreaElement.prototype : null;
            if (!proto) return {unsupported:true};
            Object.getOwnPropertyDescriptor(proto,'value').set.call(e,p.text);
            e.dispatchEvent(new Event('input',{bubbles:true}));
            e.dispatchEvent(new Event('change',{bubbles:true}));
            return {executed:a.id,readback:e.value,expected:p.text};
          }
          if (a.kind==='click') { e.click(); return {executed:a.id}; }
          return {unsupported:true};
        })(""" + json.dumps(data, ensure_ascii=False) + ")")
        if result.get("stale"):
            raise self._stale("Observed control changed or is covered")
        if result.get("unsupported") or ("readback" in result and result["readback"] != result["expected"]):
            raise RuntimeError("dom_action_readback_unconfirmed")
        return result

    def close(self):
        # This backend attaches to an existing tab; ownership remains with caller.
        return None

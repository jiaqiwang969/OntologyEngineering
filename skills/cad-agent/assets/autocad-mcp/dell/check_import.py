import sys, os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import acad_mcp_server as m
tools = m.mcp._tool_manager.list_tools()
print("tool count:", len(tools))
print(", ".join(t.name for t in tools))

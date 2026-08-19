"""行业本体内化循环统一入口；所有语义状态均由 Semantica refinery 持有。"""

import sys
import _common  # noqa: F401
from ontology_engineering.semantic_engagement import main


raise SystemExit(main(sys.argv[1:]))

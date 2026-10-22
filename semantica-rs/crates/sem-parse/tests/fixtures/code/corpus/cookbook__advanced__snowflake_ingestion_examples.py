"""
Snowflake Ingestion Examples

This module provides comprehensive examples of using the Snowflake ingestor.
"""

import os
from datetime import datetime, timedelta

from rich import box
from rich.console import Console
from rich.rule import Rule
from rich.table import Table

from semantica.ingest import SnowflakeIngestor
from semantica.utils.logging import get_logger

logger = get_logger("snowflake_examples")
console = Console()


def _section(title: str) -> None:
    console.print(Rule(f"[bold cyan]{title}[/bold cyan]", style="cyan"))


def example_basic_ingestion():
    """Example: Basic table ingestion."""
    _section("Example 1: Basic Table Ingestion")

    ingestor = SnowflakeIngestor(
        account=os.getenv("SNOWFLAKE_ACCOUNT"),
        user=os.getenv("SNOWFLAKE_USER"),
        password=os.getenv("SNOWFLAKE_PASSWORD"),
        warehouse="COMPUTE_WH",
        database="SAMPLE_DB",
        schema="PUBLIC",
    )

    data = ingestor.ingest_table("CUSTOMERS", limit=10)

    console.print(f"[green]✓[/green] Retrieved [cyan]{data.row_count}[/cyan] rows")
    console.print(f"  Columns: [dim]{data.columns}[/dim]")
    console.print(f"  First row: [dim]{data.data[0]}[/dim]")

    ingestor.close()


def example_query_execution():
    """Example: Execute custom SQL queries."""
    _section("Example 2: Query Execution")

    ingestor = SnowflakeIngestor()

    query = """
        SELECT
            COUNTRY,
            COUNT(*) AS CUSTOMER_COUNT,
            SUM(TOTAL_PURCHASES) AS TOTAL_REVENUE
        FROM CUSTOMERS
        GROUP BY COUNTRY
        ORDER BY TOTAL_REVENUE DESC
        LIMIT 10
    """

    data = ingestor.ingest_query(query)

    table = Table(title="[bold]Top 10 Countries by Revenue[/bold]",
                  box=box.SIMPLE_HEAD, show_edge=False, padding=(0, 1))
    table.add_column("Country", style="cyan", no_wrap=True)
    table.add_column("Customers", style="green", justify="right")
    table.add_column("Revenue", style="green", justify="right")
    for row in data.data:
        table.add_row(
            row["COUNTRY"],
            str(row["CUSTOMER_COUNT"]),
            f"${row['TOTAL_REVENUE']:,.2f}",
        )
    console.print(table)

    ingestor.close()


def example_parameterized_query():
    """Example: Parameterized queries."""
    _section("Example 3: Parameterized Queries")

    ingestor = SnowflakeIngestor()

    end_date = datetime.now()
    start_date = end_date - timedelta(days=30)

    query = """
        SELECT
            ORDER_ID, CUSTOMER_ID, PRODUCT_NAME, AMOUNT, ORDER_DATE
        FROM ORDERS
        WHERE ORDER_DATE BETWEEN %(start_date)s AND %(end_date)s
          AND AMOUNT > %(min_amount)s
        ORDER BY ORDER_DATE DESC
    """

    data = ingestor.ingest_query(
        query,
        params={
            "start_date": start_date.strftime("%Y-%m-%d"),
            "end_date": end_date.strftime("%Y-%m-%d"),
            "min_amount": 100.0,
        },
    )

    console.print(
        f"[green]✓[/green] Found [cyan]{data.row_count}[/cyan] orders "
        "in the last 30 days over $100"
    )
    ingestor.close()


def example_schema_introspection():
    """Example: Table schema introspection."""
    _section("Example 4: Schema Introspection")

    ingestor = SnowflakeIngestor()
    schema = ingestor.get_table_schema("CUSTOMERS")

    console.print(f"  Primary keys: [cyan]{schema['primary_keys']}[/cyan]")

    table = Table(title="[bold]CUSTOMERS Schema[/bold]",
                  box=box.SIMPLE_HEAD, show_edge=False, padding=(0, 1))
    table.add_column("Column", style="cyan", no_wrap=True)
    table.add_column("Type")
    table.add_column("Nullable")
    table.add_column("Default", style="dim")
    for col in schema["columns"]:
        table.add_row(
            col["name"],
            col["type"],
            "NULL" if col["nullable"] else "NOT NULL",
            str(col["default"]) if col["default"] else "",
        )
    console.print(table)

    ingestor.close()


def example_list_tables():
    """Example: List all tables in a schema."""
    _section("Example 5: List Tables")

    ingestor = SnowflakeIngestor()
    tables = ingestor.list_tables()

    table = Table(title=f"[bold]Tables ({len(tables)} found)[/bold]",
                  box=box.SIMPLE_HEAD, show_edge=False, padding=(0, 1))
    table.add_column("Table", style="cyan")
    for t in tables:
        table.add_row(t)
    console.print(table)

    ingestor.close()


def example_pagination():
    """Example: Paginate large result sets."""
    _section("Example 6: Pagination")

    ingestor = SnowflakeIngestor()
    PAGE_SIZE = 100
    total_rows = 0
    page = 0

    while True:
        data = ingestor.ingest_table(
            "LARGE_TABLE", limit=PAGE_SIZE, offset=page * PAGE_SIZE
        )
        if data.row_count == 0:
            break
        total_rows += data.row_count
        console.print(
            f"  [dim]Page {page + 1}:[/dim] [cyan]{data.row_count}[/cyan] rows"
        )
        process_page(data)
        page += 1

    console.print(
        f"[green]✓[/green] Total rows processed: [cyan]{total_rows}[/cyan]"
    )
    ingestor.close()


def example_batch_processing():
    """Example: Batch processing with fetchmany."""
    _section("Example 7: Batch Processing")

    ingestor = SnowflakeIngestor()
    data = ingestor.ingest_query(
        "SELECT * FROM LARGE_TABLE WHERE STATUS = 'ACTIVE'", batch_size=1000
    )
    console.print(
        f"[green]✓[/green] Retrieved [cyan]{data.row_count}[/cyan] rows "
        "in batches of 1000"
    )
    ingestor.close()


def example_export_documents():
    """Example: Export to Semantica document format."""
    _section("Example 8: Export as Documents")

    ingestor = SnowflakeIngestor()
    data = ingestor.ingest_table("PRODUCTS", limit=10)
    documents = ingestor.export_as_documents(
        data, id_field="PRODUCT_ID", text_fields=["PRODUCT_NAME", "DESCRIPTION"]
    )

    console.print(
        f"[green]✓[/green] Exported [cyan]{len(documents)}[/cyan] documents"
    )
    if documents:
        d = documents[0]
        console.print(f"  [dim]First doc — ID:[/dim] {d['id']}")
        console.print(f"  [dim]Text:[/dim] {d['text'][:100]}…")
        console.print(f"  [dim]Metadata:[/dim] {d['metadata']}")

    ingestor.close()


def example_key_pair_auth():
    """Example: Key-pair authentication."""
    _section("Example 9: Key-Pair Authentication")

    ingestor = SnowflakeIngestor(
        account=os.getenv("SNOWFLAKE_ACCOUNT"),
        user=os.getenv("SNOWFLAKE_USER"),
        private_key_path=os.getenv("SNOWFLAKE_PRIVATE_KEY_PATH"),
        warehouse="COMPUTE_WH",
    )
    data = ingestor.ingest_table("CUSTOMERS", limit=5)
    console.print(
        f"[green]✓[/green] Authenticated — retrieved [cyan]{data.row_count}[/cyan] rows"
    )
    ingestor.close()


def example_context_manager():
    """Example: Using context manager."""
    _section("Example 10: Context Manager")

    with SnowflakeIngestor() as ingestor:
        data = ingestor.ingest_table("CUSTOMERS", limit=5)
        console.print(
            f"[green]✓[/green] Retrieved [cyan]{data.row_count}[/cyan] rows"
        )
    console.print("[dim]  Connection closed automatically.[/dim]")


def example_multi_schema():
    """Example: Multi-schema ingestion."""
    _section("Example 11: Multi-Schema Ingestion")

    ingestor = SnowflakeIngestor()
    prod = ingestor.ingest_table("CUSTOMERS", database="PROD_DB", schema="PUBLIC", limit=10)
    staging = ingestor.ingest_table("CUSTOMERS", database="STAGING_DB", schema="PUBLIC", limit=10)

    console.print(f"  Production: [cyan]{prod.row_count}[/cyan] customers")
    console.print(f"  Staging:    [cyan]{staging.row_count}[/cyan] customers")

    ingestor.close()


def example_error_handling():
    """Example: Error handling."""
    _section("Example 12: Error Handling")

    from semantica.utils.exceptions import ProcessingError, ValidationError

    try:
        ingestor = SnowflakeIngestor(
            account="invalid_account", user="invalid_user", password="invalid_password"
        )
        ingestor.ingest_table("CUSTOMERS")

    except ValidationError as e:
        console.print(f"[bold yellow] ⚠[/bold yellow] Validation error: {e}")
    except ProcessingError as e:
        console.print(f"[bold red] ✗[/bold red] Processing error: {e}")
    except Exception as e:
        console.print(f"[bold red] ✗[/bold red] Unexpected error: {e}")


def example_incremental_load():
    """Example: Incremental data loading."""
    _section("Example 13: Incremental Loading")

    ingestor = SnowflakeIngestor()
    last_load = get_last_load_timestamp()

    query = """
        SELECT *
        FROM CUSTOMERS
        WHERE UPDATED_AT > %(last_load)s
        ORDER BY UPDATED_AT ASC
    """

    data = ingestor.ingest_query(query, params={"last_load": last_load})
    console.print(
        f"[green]✓[/green] Loaded [cyan]{data.row_count}[/cyan] new/updated "
        f"records since [dim]{last_load}[/dim]"
    )
    if data.row_count > 0:
        update_last_load_timestamp(datetime.now())

    ingestor.close()


def example_etl_pipeline():
    """Example: Full ETL pipeline."""
    _section("Example 14: ETL Pipeline")

    ingestor = SnowflakeIngestor()

    sales_query = """
        SELECT
            s.ORDER_ID, s.CUSTOMER_ID, c.CUSTOMER_NAME,
            s.PRODUCT_ID, p.PRODUCT_NAME, s.AMOUNT, s.ORDER_DATE
        FROM SALES s
        JOIN CUSTOMERS c ON s.CUSTOMER_ID = c.ID
        JOIN PRODUCTS p ON s.PRODUCT_ID = p.ID
        WHERE s.ORDER_DATE >= CURRENT_DATE - 7
    """

    data = ingestor.ingest_query(sales_query)
    console.print(f"  [dim]Extract:[/dim] [cyan]{data.row_count}[/cyan] sales records")

    documents = ingestor.export_as_documents(
        data, id_field="ORDER_ID", text_fields=["CUSTOMER_NAME", "PRODUCT_NAME"]
    )
    console.print(f"  [dim]Transform:[/dim] [cyan]{len(documents)}[/cyan] documents")

    from semantica.pipeline import Pipeline
    pipeline = Pipeline()
    for doc in documents:
        pipeline.process_document(doc)

    console.print("[green]✓[/green] Loaded documents into Semantica pipeline")
    ingestor.close()


# ─── Utility stubs ────────────────────────────────────────────────────────────

def process_page(data):
    pass


def get_last_load_timestamp():
    return (datetime.now() - timedelta(days=1)).strftime("%Y-%m-%d %H:%M:%S")


def update_last_load_timestamp(timestamp):
    pass


def main():
    """Run all examples."""
    examples = [
        example_basic_ingestion,
        example_query_execution,
        example_parameterized_query,
        example_schema_introspection,
        example_list_tables,
        example_export_documents,
        example_context_manager,
        example_error_handling,
    ]

    for example_func in examples:
        try:
            example_func()
            console.print()
        except Exception as e:
            logger.error("Example %s failed: %s", example_func.__name__, e)


if __name__ == "__main__":
    main()

import yaml
y = """
app_name: "Test"
styles:
  --accent-color: purple
  .search-buttons {
    display: flex;
    gap: 10px;
  }
"""
try:
    print(yaml.safe_load(y))
    print("OK")
except Exception as e:
    print(e)

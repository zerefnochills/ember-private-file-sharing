Write-Host "=== Searching JDKs ==="
Get-ChildItem -Path "C:\Program Files\Java", "C:\Program Files\Eclipse Adoptium", "C:\Users\altze\AppData\Local\Programs" -Recurse -Depth 2 -ErrorAction SilentlyContinue | Where-Object { $_.Name -like "*jdk*" -or $_.Name -like "*jbr*" } | Select-Object FullName

Write-Host "=== Searching Android Studio / JBR ==="
Get-ChildItem -Path "C:\Program Files\Android", "C:\Users\altze\AppData\Local\Android" -Recurse -Depth 3 -ErrorAction SilentlyContinue | Where-Object { $_.Name -like "*jbr*" -or $_.Name -like "*jdk*" -or $_.Name -like "*gradle*" } | Select-Object FullName

Write-Host "=== Searching .gradle cache for gradle wrappers / distributions ==="
Get-ChildItem -Path "C:\Users\altze\.gradle\wrapper\dists" -Recurse -Depth 2 -ErrorAction SilentlyContinue | Select-Object FullName

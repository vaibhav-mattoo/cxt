#!/bin/bash
echo "Generating mock repository..."
mkdir -p mock_repo && cd mock_repo
for i in {1..50}; do
  mkdir -p "dir_$i/src/components" "dir_$i/tests" "dir_$i/node_modules"
  for j in {1..1000}; do
    head -c $((RANDOM % 10240 + 100)) </dev/urandom | base64 > "dir_$i/src/components/file_$j.txt"
  done
done
cat <<EOF > .gitignore
node_modules/
*.log
temp_*
EOF
echo "Done! 50,000 files generated."

# for each .o file in data/incremental-new, `nm` it and grep for `CALLSITE`

for o in data/incremental-new/*.o; do
    nm $o | grep -i "CALLSITE"
done

echo "old..."

for o in data/incremental-old/*.o; do
    nm $o | grep -i "CALLSITE"
done

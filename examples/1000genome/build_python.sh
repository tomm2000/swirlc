chmod 777 build
rm -rf build
mkdir -p build

docker run                                        \
        --platform linux/x86_64                   \
        -u $(id -u):$(id -g)                      \
        -v $(pwd)/examples/1000genome:/data          \
        -v $(pwd)/build:/build                    \
        -w /                                      \
        swirlc-rust                               \
    swirlc                                        \
        compile                                   \
        --target default                             \
        /data/source.swirl                      \
        /data/config.yml

cd build

rm -f build.zip

# package the run.sh and .py files
zip -g build.zip run.sh *.py

cd ..

# create the folder on the server
ssh tommaso.fogliobonda@c3sfr1.di.unito.it "mkdir -p ~/swirlc-1000-genome/genome-py"

# remove the run.sh and .py files
ssh tommaso.fogliobonda@c3sfr1.di.unito.it "rm -f ~/swirlc-1000-genome/genome-py/run.sh ~/swirlc-1000-genome/genome-py/*.py"

# send the zip to c3sfr1.di.unito.it using scp
scp build/build.zip tommaso.fogliobonda@c3sfr1.di.unito.it:~/swirlc-1000-genome/genome-py/build.zip

# unzip the file on the server
ssh tommaso.fogliobonda@c3sfr1.di.unito.it "cd ~/swirlc-1000-genome/genome-py/ && unzip -o build.zip && rm build.zip"
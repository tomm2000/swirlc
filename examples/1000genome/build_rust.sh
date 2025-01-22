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
        --target rust                             \
        /data/source.swirl                      \
        /data/config.yml

# read -p "Press enter to continue"

docker run                                        \
        --platform linux/x86_64                   \
        -u $(id -u):$(id -g)                      \
        -v $(pwd)/build:/build                    \
        -w /build                                 \
        -e RUSTFLAGS="-A warnings"                \
        swirlc-rust                               \
    cargo build --release

cd build

dos2unix run.sh

rm -f build.zip

# package the run.sh and target/release/swirlc-rust into a zip file
cd target/release
zip ../../build.zip swirlc-rust
cd ../..
zip -g build.zip run.sh

cd ..

# create the folder on the server
ssh tommaso.fogliobonda@c3sfr1.di.unito.it "mkdir -p ~/swirlc-1000-genome/genome"

# send the zip to c3sfr1.di.unito.it using scp
scp build/build.zip tommaso.fogliobonda@c3sfr1.di.unito.it:~/swirlc-1000-genome/genome/build.zip

# unzip the file on the server
ssh tommaso.fogliobonda@c3sfr1.di.unito.it "cd ~/swirlc-1000-genome/genome/ && unzip -o build.zip && rm build.zip"
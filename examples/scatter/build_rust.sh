mkdir -p build
chmod 777 build

docker run                                        \
        --platform linux/x86_64                   \
        -u $(id -u):$(id -g)                      \
        -v $(pwd)/examples/scatter:/data          \
        -v $(pwd)/build:/build                    \
        -w /                                      \
        swirlc-rust                               \
    swirlc                                        \
        compile                                   \
        --target rust                             \
        /data/source.swirl                      \
        /data/config.yml

read -p "Press enter to continue"

docker run                                        \
        --platform linux/x86_64                   \
        -u $(id -u):$(id -g)                      \
        -v $(pwd)/build:/build                    \
        -w /build                                 \
        -e RUSTFLAGS="-A warnings"                \
        swirlc-rust                               \
    cargo build --release

cd build

# ssh tommaso.fogliobonda@c3sfr1.di.unito.it 'rm -rf ~/swirlc-1000-genome/src'
# scp -r ./build/src/ tommaso.fogliobonda@c3sfr1.di.unito.it:~/swirlc-1000-genome/src/

scp ./Cargo.toml tommaso.fogliobonda@c3sfr1.di.unito.it:~/swirlc-1000-genome/Cargo.toml
scp ./run.sh tommaso.fogliobonda@c3sfr1.di.unito.it:~/swirlc-1000-genome/run.sh

ssh tommaso.fogliobonda@c3sfr1.di.unito.it 'rm -rf ~/swirlc-1000-genome/target'
ssh tommaso.fogliobonda@c3sfr1.di.unito.it 'mkdir -p ~/swirlc-1000-genome/target/release'

# zip the target/release/location* files (ignore locaition*.d files)
zip -r location.zip target/release/location* -x "*.d"
# send the zip file to the server
scp location.zip tommaso.fogliobonda@c3sfr1.di.unito.it:~/swirlc-1000-genome/location.zip
# remove the zip file
rm location.zip
# unzip the zip file on the server
ssh tommaso.fogliobonda@c3sfr1.di.unito.it 'unzip -o ~/swirlc-1000-genome/location.zip -d ~/swirlc-1000-genome/'
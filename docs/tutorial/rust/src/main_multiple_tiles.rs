// Copyright © SixtyFPS GmbH <info@slint-ui.com>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-1.0 OR LicenseRef-Slint-commercial

#[allow(dead_code)]
fn main() {
    MainWindow::new().unwrap().run().unwrap();
}
slint::slint! {
// ANCHOR: tile_data

// Added:
struct TileData {
    image: image,
    image_visible: bool,
    solved: bool,
}

component MemoryTile inherits Rectangle {

// ANCHOR_END: tile_data
    callback clicked;
    in property <bool> open_curtain;
    in property <bool> solved;
    in property <image> icon;

    height: 64px;
    width: 64px;
    background: solved ? #34CE57 : #3960D5;
    animate background { duration: 800ms; }

    Image {
        source: icon;
        width: parent.width;
        height: parent.height;
    }

    // Left curtain
    Rectangle {
        background: #193076;
        width: open_curtain ? 0px : (parent.width / 2);
        height: parent.height;
        animate width { duration: 250ms; easing: ease-in; }
    }

    // Right curtain
    Rectangle {
        background: #193076;
        x: open_curtain ? parent.width : (parent.width / 2);
        width: open_curtain ? 0px : (parent.width / 2);
        height: parent.height;
        animate width { duration: 250ms; easing: ease-in; }
        animate x { duration: 250ms; easing: ease-in; }
    }

    TouchArea {
        clicked => {
            // Delegate to the user of this element
            root.clicked();
        }
    }
}
// ANCHOR: main_window
export component MainWindow inherits Window {
    width: 326px;
    height: 326px;

    in property <[TileData]> memory_tiles: [
        { image: @image-url("icons/at.png") },
        { image: @image-url("icons/balance-scale.png") },
        { image: @image-url("icons/bicycle.png") },
        { image: @image-url("icons/bus.png") },
        { image: @image-url("icons/cloud.png") },
        { image: @image-url("icons/cogs.png") },
        { image: @image-url("icons/motorcycle.png") },
        { image: @image-url("icons/video.png") },
    ];
    for tile[i] in memory_tiles : MemoryTile {
        x: mod(i, 4) * 74px;
        y: floor(i / 4) * 74px;
        width: 64px;
        height: 64px;
        icon: tile.image;
        open_curtain: tile.image_visible || tile.solved;
        // propagate the solved status from the model to the tile
        solved: tile.solved;
        clicked => {
            tile.image_visible = !tile.image_visible;
        }
    }
}
// ANCHOR_END: main_window
}

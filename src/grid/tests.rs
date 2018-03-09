// Copyright 2016 Joe Wilm, The Alacritty Project Contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Tests for the Gird

use super::{Grid, BidirectionalIterator};
use index::{Point, Line, Column};

// Scroll up moves lines upwards
#[test]
fn scroll_up() {
    info!("");

    let mut grid = Grid::new(Line(10), Column(1), 0, 0);
    for i in 0..10 {
        grid[Line(i)][Column(0)] = i;
    }

    info!("grid: {:?}", grid);

    grid.scroll_up(&(Line(0)..Line(10)), Line(2));

    info!("grid: {:?}", grid);

    let mut other = Grid::new(Line(10), Column(1), 0, 9);

    other[Line(0)][Column(0)] = 2;
    other[Line(1)][Column(0)] = 3;
    other[Line(2)][Column(0)] = 4;
    other[Line(3)][Column(0)] = 5;
    other[Line(4)][Column(0)] = 6;
    other[Line(5)][Column(0)] = 7;
    other[Line(6)][Column(0)] = 8;
    other[Line(7)][Column(0)] = 9;
    other[Line(8)][Column(0)] = 0;
    other[Line(9)][Column(0)] = 1;

    for i in 0..10 {
        assert_eq!(grid[Line(i)][Column(0)], other[Line(i)][Column(0)]);
    }
}

// Scroll down moves lines downwards
#[test]
fn scroll_down() {
    info!("");

    let mut grid = Grid::new(Line(10), Column(1), 0, 0);
    for i in 0..10 {
        grid[Line(i)][Column(0)] = i;
    }

    info!("grid: {:?}", grid);

    grid.scroll_down(&(Line(0)..Line(10)), Line(2));

    info!("grid: {:?}", grid);

    let mut other = Grid::new(Line(10), Column(1), 0, 9);

    other[Line(0)][Column(0)] = 8;
    other[Line(1)][Column(0)] = 9;
    other[Line(2)][Column(0)] = 0;
    other[Line(3)][Column(0)] = 1;
    other[Line(4)][Column(0)] = 2;
    other[Line(5)][Column(0)] = 3;
    other[Line(6)][Column(0)] = 4;
    other[Line(7)][Column(0)] = 5;
    other[Line(8)][Column(0)] = 6;
    other[Line(9)][Column(0)] = 7;

    for i in 0..10 {
        assert_eq!(grid[Line(i)][Column(0)], other[Line(i)][Column(0)]);
    }
}

// Test that GridIterator works
#[test]
fn test_iter() {
    info!("");

    let mut grid = Grid::new(Line(5), Column(5), 0, 0);
    for i in 0..5 {
        for j in 0..5 {
            grid[Line(i)][Column(j)] = i*5 + j;
        }
    }

    info!("grid: {:?}", grid);

    let mut iter = grid.iter_from(Point {
        line: 4,
        col: Column(0),
    });

    assert_eq!(None, iter.prev());
    assert_eq!(Some(&1), iter.next());
    assert_eq!(Column(1), iter.cur.col);
    assert_eq!(4, iter.cur.line);

    assert_eq!(Some(&2), iter.next());
    assert_eq!(Some(&3), iter.next());
    assert_eq!(Some(&4), iter.next());

    // test linewrapping
    assert_eq!(Some(&5), iter.next());
    assert_eq!(Column(0), iter.cur.col);
    assert_eq!(3, iter.cur.line);

    assert_eq!(Some(&4), iter.prev());
    assert_eq!(Column(4), iter.cur.col);
    assert_eq!(4, iter.cur.line);


    // test that iter ends at end of grid
    let mut final_iter = grid.iter_from(Point {
        line: 0,
        col: Column(4),
    });
    assert_eq!(None, final_iter.next());
    assert_eq!(Some(&23), final_iter.prev());
}

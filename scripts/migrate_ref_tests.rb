#!/usr/bin/env ruby

require 'json'

Dir.glob('./tests/ref/**/grid.json').each do |path|
  puts "Migrating #{path}"

  # Read contents
  s = File.open(path) { |f| f.read }

  # Parse
  grid = JSON.parse(s)

  # Normalize Storage serialization
  if grid['raw'].is_a? Array
    grid['raw'] = {
      'inner' => grid['raw'][0],
      'zero' => grid['raw'][1],
      'visible_lines' => grid['raw'][2]
    }
  end

  # Migrate Row serialization
  grid['raw']['inner'].map! do |row|
    if row.is_a? Hash
      row
    else
      { inner: row, occ: row.length }
    end
  end

  # Write updated grid
  File.open(path, 'w') { |f| f << JSON.generate(grid) }
end

#!/usr/bin/env ruby

require 'json'

Dir.glob('./tests/ref/**/grid.json').each do |path|
  # Read contents
  s = File.open(path) { |f| f.read }

  # Parse
  grid = JSON.parse(s)

  # Check if it's already migrated / make this migration idempotent
  next if grid['raw'][0][0].is_a? Array

  # Transform
  grid['raw'].reverse!
  grid['raw'] = [grid['raw'], 0, grid['lines'] - 1]

  # Write updated grid
  File.open(path, 'w') { |f| f << JSON.generate(grid) }
end
